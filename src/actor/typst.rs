use std::sync::Arc;

use crate::{ChangeCursorPositionRequest, MemoryFiles, MemoryFilesShort, SrcToDocJumpRequest};
use log::{debug, error, info};
use tokio::sync::{broadcast, mpsc, watch};
use typst::{doc::Document, World};
use typst_ts_compiler::service::CompileDriver;
use typst_ts_compiler::service::{CompileActor, CompileClient as TsCompileClient, CompileExporter};
use typst_ts_compiler::vfs::notify::{FileChangeSet, MemoryEvent};

use super::render::RenderActorRequest;
use super::{
    editor::EditorActorRequest,
    webview::{SrcToDocJumpInfo, WebviewActorRequest},
};

#[derive(Debug)]
pub enum TypstActorRequest {
    DocToSrcJumpResolve(u64),
    ChangeCursorPosition(ChangeCursorPositionRequest),
    SrcToDocJumpResolve(SrcToDocJumpRequest),

    SyncMemoryFiles(MemoryFiles),
    UpdateMemoryFiles(MemoryFiles),
    RemoveMemoryFiles(MemoryFilesShort),
}

type CompileService = CompileActor<CompileExporter<CompileDriver>>;
type CompileClient = TsCompileClient<CompileService>;

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
    pub doc_to_src_jump: MpScChannel<EditorActorRequest>,
    pub src_to_doc_jump: BroadcastChannel<WebviewActorRequest>,
}

impl TypstActor {
    pub fn set_up_channels() -> Channels {
        let typst_mailbox = mpsc::unbounded_channel();
        let doc_watch = watch::channel(None);
        let renderer_mailbox = broadcast::channel(32);
        let doc_to_src_jump = mpsc::unbounded_channel();
        let src_to_doc_jump = broadcast::channel(32);
        Channels {
            typst_mailbox,
            doc_watch,
            renderer_mailbox,
            doc_to_src_jump,
            src_to_doc_jump,
        }
    }

    pub fn new(
        compiler_driver: CompileDriver,
        mailbox: mpsc::UnboundedReceiver<TypstActorRequest>,
        doc_sender: watch::Sender<Option<Arc<Document>>>,
        renderer_sender: broadcast::Sender<RenderActorRequest>,
        doc_to_src_jump_sender: mpsc::UnboundedSender<EditorActorRequest>,
        src_to_doc_jump_sender: broadcast::Sender<WebviewActorRequest>,
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
        let inner = CompileActor::new(driver, root.as_ref().to_owned()).with_watch(true);

        Self {
            inner,
            client: TypstClient {
                inner: once_cell::sync::OnceCell::new(),
                mailbox,
                doc_to_src_jump_sender,
                src_to_doc_jump_sender,
            },
        }
    }

    pub async fn run(self) {
        let (server, client) = self.inner.split();
        server.spawn().await;

        if self.client.inner.set(client).is_err() {
            unreachable!();
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

    doc_to_src_jump_sender: mpsc::UnboundedSender<EditorActorRequest>,
    src_to_doc_jump_sender: broadcast::Sender<WebviewActorRequest>,
}

impl TypstClient {
    fn inner(&mut self) -> &mut CompileClient {
        self.inner.get_mut().unwrap()
    }

    async fn process_mail(&mut self, mail: TypstActorRequest) {
        match mail {
            TypstActorRequest::DocToSrcJumpResolve(id) => {
                debug!("TypstActor: processing doc2src: {:?}", id);

                let res = self
                    .inner()
                    .resolve_doc_to_src_jump(id)
                    .await
                    .map_err(|err| {
                        error!("TypstActor: failed to resolve doc to src jump: {:#}", err);
                    })
                    .ok()
                    .flatten();

                if let Some(info) = res {
                    let _ = self
                        .doc_to_src_jump_sender
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
                    .flatten()
                    .map(|jump_pos| SrcToDocJumpInfo {
                        page_no: jump_pos.page.into(),
                        x: jump_pos.point.x.to_pt(),
                        y: jump_pos.point.y.to_pt(),
                    });

                if let Some(info) = res {
                    let _ = self
                        .src_to_doc_jump_sender
                        .send(WebviewActorRequest::CursorPosition(info));
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
                    .flatten()
                    .map(|jump_pos| SrcToDocJumpInfo {
                        page_no: jump_pos.page.into(),
                        x: jump_pos.point.x.to_pt(),
                        y: jump_pos.point.y.to_pt(),
                    });

                if let Some(info) = res {
                    let _ = self
                        .src_to_doc_jump_sender
                        .send(WebviewActorRequest::SrcToDocJump(info));
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
