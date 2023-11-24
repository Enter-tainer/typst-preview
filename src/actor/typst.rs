use std::sync::Arc;

use crate::{ChangeCursorPositionRequest, MemoryFiles, MemoryFilesShort, SrcToDocJumpRequest};
use log::{debug, error, info};
use tokio::sync::{broadcast, mpsc, watch};
use typst::diag::SourceResult;
use typst::doc::{Frame, FrameItem, Meta};
use typst::geom::{Geometry, Point, Size};
use typst::syntax::Span;
use typst::{doc::Document, World};
use typst_ts_compiler::service::{
    CompileActor, CompileClient as TsCompileClient, CompileExporter, Compiler, WorldExporter,
};
use typst_ts_compiler::service::{CompileDriver, WrappedCompiler};
use typst_ts_compiler::vfs::notify::{FileChangeSet, MemoryEvent};
use typst_ts_core::vector::span_id_to_u64;

use super::debug_loc::DocumentPosition;
use super::editor::CompileStatus;
use super::render::RenderActorRequest;
use super::{editor::EditorActorRequest, webview::WebviewActorRequest};

#[derive(Debug)]
pub enum TypstActorRequest {
    JumpToSrcByPosition(DocumentPosition),
    DocToSrcJumpResolve(/* span id */ u64),
    ChangeCursorPosition(ChangeCursorPositionRequest),
    SrcToDocJumpResolve(SrcToDocJumpRequest),

    SyncMemoryFiles(MemoryFiles),
    UpdateMemoryFiles(MemoryFiles),
    RemoveMemoryFiles(MemoryFilesShort),
}

type CompileService = CompileActor<Reporter<CompileExporter<CompileDriver>>>;
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
    pub editor_conn: MpScChannel<EditorActorRequest>,
    pub webview_conn: BroadcastChannel<WebviewActorRequest>,
}

struct Reporter<C> {
    inner: C,
    sender: mpsc::UnboundedSender<EditorActorRequest>,
}

impl<C: Compiler> WrappedCompiler for Reporter<C> {
    type Compiler = C;

    fn inner(&self) -> &Self::Compiler {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut Self::Compiler {
        &mut self.inner
    }

    fn wrap_compile(&mut self) -> SourceResult<Arc<Document>> {
        let _ = self
            .sender
            .send(EditorActorRequest::CompileStatus(CompileStatus::Compiling));
        let doc = self.inner_mut().compile();
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
    fn export(&mut self, output: Arc<typst::doc::Document>) -> SourceResult<()> {
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
            TypstActorRequest::DocToSrcJumpResolve(id) => {
                debug!("TypstActor: processing doc2src: {:?}", id);
                self.doc2src(id).await;
            }
            TypstActorRequest::JumpToSrcByPosition(pos) => {
                debug!("TypstActor: processing jump to src by position: {pos:?}");

                // todo: non-main file
                let res = self
                    .inner()
                    .steal_async(move |this, _| {
                        let doc = this.document()?;
                        let frame = doc.pages.get(pos.page_no - 1)?;

                        let main_id = this.compiler.main_id();
                        let main_src = this.compiler.world().source(main_id).ok()?;
                        let filter_by_main =
                            |s: &Span| s.id() == Some(main_id) && main_src.find(*s).is_some();

                        find_in_frame(frame, pos.point(), &filter_by_main)
                            .or_else(|| {
                                // unfortunately, the heading element is not in the main file

                                // find all locations in the frame and in the main file
                                let mut locations = vec![];
                                let mut lines = vec![];
                                let fuzzy_collector = &mut |_pos: &Point, s: &Span| {
                                    if s.id() != Some(main_id) {
                                        return false;
                                    }
                                    let Some(node) = main_src.find(*s) else {
                                        return false;
                                    };

                                    let loc = node.offset();
                                    let line = main_src.byte_to_line(loc).unwrap_or_default();
                                    lines.push(line);

                                    locations.push(((line, loc), *s));
                                    true
                                };
                                fuzzy_find_in_frame(frame, fuzzy_collector);
                                // locations.sort_by_key(|((_, loc), _)| *loc);
                                // debug!("TypstActor: fuzzy locations: {:?}", locations);

                                // sort line numbers to find the last contiguous lines
                                lines.sort();

                                // calculate reasonable delta bound
                                let average_delta =
                                    lines.windows(2).map(|w| w[1] - w[0]).sum::<usize>()
                                        / lines.len();
                                let average_sigma = (lines
                                    .windows(2)
                                    .map(|w| (w[1] - w[0] - average_delta).pow(2) as f32)
                                    .sum::<f32>()
                                    / lines.len() as f32)
                                    .sqrt()
                                    .ceil()
                                    as usize;
                                let reasonable_delta_bound = average_delta + average_sigma * 2;

                                // pick the last contiguous lines
                                let mut pick_line = lines.last().cloned().unwrap_or_default();
                                for line in lines.iter().rev() {
                                    if pick_line - *line > reasonable_delta_bound {
                                        break;
                                    }
                                    pick_line = *line;
                                }

                                // min location in the last contiguous lines
                                let picked = locations
                                    .into_iter()
                                    .filter(|((line, _), _)| *line == pick_line)
                                    .min_by_key(|((_, loc), _)| *loc);

                                debug!(
                                    "TypstActor: picked location: {:?}(bar = {:?}) {:?}",
                                    pick_line, reasonable_delta_bound, picked
                                );
                                picked.map(|(_, s)| s)
                            })
                            .map(|s| span_id_to_u64(&s))
                    })
                    .await
                    .map_err(|err| {
                        error!("TypstActor: failed to resolve page position: {:#}", err);
                    })
                    .ok()
                    .flatten();

                if let Some(id) = res {
                    self.doc2src(id).await;
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

    async fn doc2src(&mut self, id: u64) {
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
                .editor_conn_sender
                .send(EditorActorRequest::DocToSrcJump(info));
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

/// Find the position of a span in a frame.
fn fuzzy_find_in_frame(frame: &Frame, filter: &mut impl FnMut(&Point, &Span) -> bool) -> bool {
    let mut res = false;
    for (mut pos, item) in frame.items().rev() {
        match item {
            FrameItem::Group(group) => {
                // TODO: Handle transformation.
                res = fuzzy_find_in_frame(&group.frame, filter) || res;
            }
            FrameItem::Meta(Meta::Elem(elem), _) if filter(&pos, &elem.span()) => {
                res = true;
            }
            FrameItem::Text(text) => {
                for glyph in &text.glyphs {
                    let width = glyph.x_advance.at(text.size);
                    if filter(&pos, &glyph.span.0) {
                        res = true;
                    }

                    pos.x += width;
                }
            }
            FrameItem::Shape(_, span) | FrameItem::Image(_, _, span) => {
                if filter(&pos, span) {
                    res = true;
                }
            }

            _ => {}
        }
    }

    res
}

/// Find the position of a span in a frame.
fn find_in_frame(frame: &Frame, click: Point, filter: &impl Fn(&Span) -> bool) -> Option<Span> {
    for (mut pos, item) in frame.items() {
        match item {
            FrameItem::Group(group) => {
                // TODO: Handle transformation.
                if let Some(span) = find_in_frame(&group.frame, click - pos, filter) {
                    return Some(span);
                }
            }
            FrameItem::Meta(Meta::Elem(elem), _) if filter(&elem.span()) => {
                return Some(elem.span());
            }
            FrameItem::Text(text) => {
                for glyph in &text.glyphs {
                    let width = glyph.x_advance.at(text.size);
                    if is_in_rect(
                        Point::new(pos.x, pos.y - text.size),
                        Size::new(width, text.size),
                        click,
                    ) && filter(&glyph.span.0)
                    {
                        return Some(glyph.span.0);
                    }

                    pos.x += width;
                }
            }

            FrameItem::Shape(shape, span) => {
                let Geometry::Rect(size) = shape.geometry else {
                    continue;
                };
                if is_in_rect(pos, size, click) && filter(span) {
                    return Some(*span);
                }
            }

            FrameItem::Image(_, size, span) if is_in_rect(pos, *size, click) && filter(span) => {
                return Some(*span);
            }

            _ => {}
        }
    }

    None
}

/// Whether a rectangle with the given size at the given position contains the
/// click position.
fn is_in_rect(pos: Point, size: Size, click: Point) -> bool {
    pos.x <= click.x && pos.x + size.x >= click.x && pos.y <= click.y && pos.y + size.y >= click.y
}
