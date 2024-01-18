use std::sync::Arc;

use crate::{ChangeCursorPositionRequest, MemoryFiles, MemoryFilesShort, SrcToDocJumpRequest};
use log::{debug, error, info};
use tokio::sync::{broadcast, mpsc, watch};
use typst::diag::SourceResult;
use typst::syntax::Span;
use typst::{model::Document, World};
use typst_ts_compiler::service::{
    CompileActor, CompileClient as TsCompileClient, CompileExporter, Compiler, DocToSrcJumpInfo,
    WorldExporter,
};
use typst_ts_compiler::service::{CompileDriver, CompileMiddleware};
use typst_ts_compiler::vfs::notify::{FileChangeSet, MemoryEvent};
use typst_ts_core::debug_loc::SourceSpanOffset;

use super::editor::CompileStatus;
use super::render::RenderActorRequest;
use super::{editor::EditorActorRequest, webview::WebviewActorRequest};

#[derive(Debug)]
pub enum TypstActorRequest {
    DocToSrcJumpResolve((SourceSpanOffset, SourceSpanOffset)),
    ChangeCursorPosition(ChangeCursorPositionRequest),
    SrcToDocJumpResolve(SrcToDocJumpRequest),

    SyncMemoryFiles(MemoryFiles),
    UpdateMemoryFiles(MemoryFiles),
    RemoveMemoryFiles(MemoryFilesShort),
}

pub type CompileService = CompileActor<Reporter<CompileExporter<CompileDriver>>>;
pub type CompileClient = TsCompileClient<CompileService>;

pub struct TypstActor {
    inner: CompileService,
    client: TypstClient,
}

type MpScChannel<T> = (mpsc::UnboundedSender<T>, mpsc::UnboundedReceiver<T>);
type WatchChannel<T> = (watch::Sender<T>, watch::Receiver<T>);
type BroadcastChannel<T> = (broadcast::Sender<T>, broadcast::Receiver<T>);

pub struct Channels {
    pub typst_mailbox: MpScChannel<TypstActorRequest>,
    pub doc_watch: WatchChannel<Option<Arc<Document>>>,
    pub renderer_mailbox: BroadcastChannel<RenderActorRequest>,
    pub editor_conn: MpScChannel<EditorActorRequest>,
    pub webview_conn: BroadcastChannel<WebviewActorRequest>,
}

pub struct Reporter<C> {
    inner: C,
    sender: mpsc::UnboundedSender<EditorActorRequest>,
}

impl<C: Compiler> CompileMiddleware for Reporter<C> {
    type Compiler = C;

    fn inner(&self) -> &Self::Compiler {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut Self::Compiler {
        &mut self.inner
    }

    fn wrap_compile(
        &mut self,
        env: &mut typst_ts_compiler::service::CompileEnv,
    ) -> SourceResult<Arc<Document>> {
        let _ = self
            .sender
            .send(EditorActorRequest::CompileStatus(CompileStatus::Compiling));
        let doc = self.inner_mut().compile(env);
        if let Err(err) = &doc {
            let _ = self.sender.send(EditorActorRequest::CompileStatus(
                CompileStatus::CompileError,
            ));
            log::error!("TypstActor: compile error: {:?}", err);
        } else {
            let _ = self.sender.send(EditorActorRequest::CompileStatus(
                CompileStatus::CompileSuccess,
            ));
        }

        doc
    }
}

impl<C: Compiler + WorldExporter> WorldExporter for Reporter<C> {
    fn export(&mut self, output: Arc<typst::model::Document>) -> SourceResult<()> {
        self.inner.export(output)
    }
}

impl TypstActor {
    pub fn set_up_channels() -> Channels {
        let typst_mailbox = mpsc::unbounded_channel();
        let doc_watch = watch::channel(None);
        let renderer_mailbox = broadcast::channel(1024);
        let editor_conn = mpsc::unbounded_channel();
        let webview_conn = broadcast::channel(32);
        Channels {
            typst_mailbox,
            doc_watch,
            renderer_mailbox,
            editor_conn,
            webview_conn,
        }
    }

    pub fn new(
        compiler_driver: CompileDriver,
        mailbox: mpsc::UnboundedReceiver<TypstActorRequest>,
        doc_sender: watch::Sender<Option<Arc<Document>>>,
        renderer_sender: broadcast::Sender<RenderActorRequest>,
        editor_conn_sender: mpsc::UnboundedSender<EditorActorRequest>,
        webview_conn_sender: broadcast::Sender<WebviewActorRequest>,
    ) -> Self {
        // CompileExporter + DynamicLayoutCompiler + WatchDriver
        let root = compiler_driver.world.root.clone();
        let driver = CompileExporter::new(compiler_driver).with_exporter(
            move |_world: &dyn World, doc: Arc<Document>| {
                let _ = doc_sender.send(Some(doc)); // it is ok to ignore the error here
                let _ = renderer_sender.send(RenderActorRequest::RenderIncremental);
                Ok(())
            },
        );
        let driver = Reporter {
            inner: driver,
            sender: editor_conn_sender.clone(),
        };
        let inner = CompileActor::new(driver, root.as_ref().to_owned()).with_watch(true);

        Self {
            inner,
            client: TypstClient {
                inner: once_cell::sync::OnceCell::new(),
                mailbox,
                editor_conn_sender,
                webview_conn_sender,
            },
        }
    }

    pub async fn run(self) {
        let (server, client) = self.inner.split();
        server.spawn().await;

        if self.client.inner.set(client).is_err() {
            panic!("TypstActor: failed to set client");
        }

        let mut client = self.client;

        debug!("TypstActor: waiting for message");
        while let Some(mail) = client.mailbox.recv().await {
            client.process_mail(mail).await;
        }
        info!("TypstActor: exiting");
    }
}

struct TypstClient {
    inner: once_cell::sync::OnceCell<CompileClient>,

    mailbox: mpsc::UnboundedReceiver<TypstActorRequest>,

    editor_conn_sender: mpsc::UnboundedSender<EditorActorRequest>,
    webview_conn_sender: broadcast::Sender<WebviewActorRequest>,
}

impl TypstClient {
    fn inner(&mut self) -> &mut CompileClient {
        self.inner.get_mut().unwrap()
    }

    async fn process_mail(&mut self, mail: TypstActorRequest) {
        match mail {
            TypstActorRequest::DocToSrcJumpResolve(span_range) => {
                debug!("TypstActor: processing doc2src: {:?}", span_range);
                let res = self.resolve_span_range(span_range).await;

                if let Some(info) = res {
                    let _ = self
                        .editor_conn_sender
                        .send(EditorActorRequest::DocToSrcJump(info));
                }
            }
            TypstActorRequest::ChangeCursorPosition(req) => {
                debug!("TypstActor: processing src2doc: {:?}", req);

                // todo: change name to resolve resolve src position
                let res = self
                    .inner()
                    .resolve_src_to_doc_jump(req.filepath, req.line, req.character)
                    .await
                    .map_err(|err| {
                        error!("TypstActor: failed to resolve cursor position: {:#}", err);
                    })
                    .ok()
                    .flatten();

                if let Some(info) = res {
                    let _ = self
                        .webview_conn_sender
                        .send(WebviewActorRequest::CursorPosition(info.into()));
                }
            }
            TypstActorRequest::SrcToDocJumpResolve(req) => {
                debug!("TypstActor: processing src2doc: {:?}", req);

                let res = self
                    .inner()
                    .resolve_src_to_doc_jump(req.filepath, req.line, req.character)
                    .await
                    .map_err(|err| {
                        error!("TypstActor: failed to resolve src to doc jump: {:#}", err);
                    })
                    .ok()
                    .flatten();

                if let Some(info) = res {
                    let _ = self
                        .webview_conn_sender
                        .send(WebviewActorRequest::SrcToDocJump(info.into()));
                }
            }
            TypstActorRequest::SyncMemoryFiles(m) => {
                debug!(
                    "TypstActor: processing SYNC memory files: {:?}",
                    m.files.keys().collect::<Vec<_>>()
                );
                self.update_memory_files(m, true);
            }
            TypstActorRequest::UpdateMemoryFiles(m) => {
                debug!(
                    "TypstActor: processing UPDATE memory files: {:?}",
                    m.files.keys().collect::<Vec<_>>()
                );
                self.update_memory_files(m, false);
            }
            TypstActorRequest::RemoveMemoryFiles(m) => {
                debug!("TypstActor: processing REMOVE memory files: {:?}", m.files);
                self.remove_shadow_files(m);
            }
        }
    }

    async fn resolve_span(&mut self, s: Span, offset: Option<usize>) -> Option<DocToSrcJumpInfo> {
        self.inner()
            .resolve_span_and_offset(s, offset)
            .await
            .map_err(|err| {
                error!("TypstActor: failed to resolve doc to src jump: {:#}", err);
            })
            .ok()
            .flatten()
    }

    async fn resolve_span_offset(&mut self, s: SourceSpanOffset) -> Option<DocToSrcJumpInfo> {
        self.resolve_span(s.span, Some(s.offset)).await
    }

    async fn resolve_span_range(
        &mut self,
        span_range: (SourceSpanOffset, SourceSpanOffset),
    ) -> Option<DocToSrcJumpInfo> {
        // Resolves FileLoC of start, end, and the element wide
        let st_res = self.resolve_span_offset(span_range.0).await;
        let ed_res = self.resolve_span_offset(span_range.1).await;
        let elem_res = self.resolve_span(span_range.1.span, None).await;

        // Combines the result of start and end
        let range_res = match (st_res, ed_res) {
            (Some(st), Some(ed)) => {
                if st.filepath == ed.filepath
                    && matches!((&st.start, &st.end), (Some(x), Some(y)) if x <= y)
                {
                    Some(DocToSrcJumpInfo {
                        filepath: st.filepath,
                        start: st.start,
                        end: ed.start,
                    })
                } else {
                    Some(ed)
                }
            }
            (Some(e), None) | (None, Some(e)) => Some(e),
            (None, None) => None,
        };

        // Account for the case where the start and end are out of order.
        //
        // This could happen because typst supports scripting, which makes text out of
        // order
        let range_res = {
            let mut range_res = range_res;
            if let Some(info) = &mut range_res {
                if let Some((x, y)) = info.start.zip(info.end) {
                    if y <= x {
                        std::mem::swap(&mut info.start, &mut info.end);
                    }
                }
            }

            range_res
        };

        // Restricts the range to the element's range
        match (elem_res, range_res) {
            (Some(elem), Some(mut rng)) if elem.filepath == rng.filepath => {
                // Account for the case where the element's range is out of order.
                let elem_start = elem.start.or(elem.end);
                let elem_end = elem.end.or(elem_start);

                // Account for the case where the range is out of order.
                let rng_start = rng.start.or(rng.end);
                let rng_end = rng.end.or(rng_start);

                if let Some((((u, inner_u), inner_v), v)) =
                    elem_start.zip(rng_start).zip(rng_end).zip(elem_end)
                {
                    rng.start = Some(inner_u.max(u).min(v));
                    rng.end = Some(inner_v.max(u).min(v));
                }
                Some(rng)
            }
            (.., Some(e)) | (Some(e), None) => Some(e),
            (None, None) => None,
        }
    }

    fn update_memory_files(&mut self, files: MemoryFiles, reset_shadow: bool) {
        // todo: is it safe to believe that the path is normalized?
        let now = std::time::SystemTime::now();
        let files = FileChangeSet::new_inserts(
            files
                .files
                .into_iter()
                .map(|(path, content)| {
                    let content = content.as_bytes().into();
                    // todo: cloning PathBuf -> Arc<Path>
                    (path.into(), Ok((now, content)).into())
                })
                .collect(),
        );
        self.inner().add_memory_changes(if reset_shadow {
            MemoryEvent::Sync(files)
        } else {
            MemoryEvent::Update(files)
        });
    }

    fn remove_shadow_files(&mut self, files: MemoryFilesShort) {
        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_removes(files.files.into_iter().map(From::from).collect());
        self.inner().add_memory_changes(MemoryEvent::Update(files))
    }
}
