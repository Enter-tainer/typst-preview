use std::{path::Path, sync::Arc};

use crate::{
    jump_from_cursor, DocToSrcJumpInfo, MemoryFiles, MemoryFilesShort, SrcToDocJumpRequest,
};
use log::{debug, error, info};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{broadcast, mpsc, watch};
use typst::syntax::{FileId, VirtualPath};
use typst::{doc::Document, World};
use typst_ts_compiler::{
    service::{CompileDriver, Compiler, DiagObserver},
    ShadowApi,
};

use super::render::RenderActorRequest;
use super::{
    editor::EditorActorRequest,
    webview::{SrcToDocJumpInfo, WebviewActorRequest},
};

#[derive(Debug)]
pub enum TypstActorRequest {
    DocToSrcJumpResolve(u64),
    SrcToDocJumpResolve(SrcToDocJumpRequest),

    SyncMemoryFiles(MemoryFiles),
    UpdateMemoryFiles(MemoryFiles),
    RemoveMemoryFiles(MemoryFilesShort),

    FilesystemEvent(notify::Event),
}

pub struct TypstActor {
    compiler_driver: CompileDriver,
    fs_watcher: RecommendedWatcher,
    mailbox: mpsc::UnboundedReceiver<TypstActorRequest>,

    doc_sender: watch::Sender<Option<Arc<Document>>>,
    renderer_sender: broadcast::Sender<RenderActorRequest>,
    doc_to_src_jump_sender: mpsc::UnboundedSender<EditorActorRequest>,
    src_to_doc_jump_sender: broadcast::Sender<WebviewActorRequest>,
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
        mailbox_sender: mpsc::UnboundedSender<TypstActorRequest>,
        doc_sender: watch::Sender<Option<Arc<Document>>>,
        renderer_sender: broadcast::Sender<RenderActorRequest>,
        doc_to_src_jump_sender: mpsc::UnboundedSender<EditorActorRequest>,
        src_to_doc_jump_sender: broadcast::Sender<WebviewActorRequest>,
    ) -> Self {
        let fs_watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, _>| match res {
                Ok(e) => {
                    info!("TypstActor: filesystem event: {:?}", e);
                    mailbox_sender
                        .send(TypstActorRequest::FilesystemEvent(e))
                        .unwrap();
                }
                Err(e) => error!("watch error: {:#}", e),
            },
            notify::Config::default(),
        );
        let fs_watcher = match fs_watcher {
            Ok(w) => w,
            Err(e) => {
                error!("TypstActor: failed to create filesystem watcher: {:#}", e);
                panic!();
            }
        };

        Self {
            compiler_driver,
            fs_watcher,
            mailbox,
            doc_sender,
            renderer_sender,
            doc_to_src_jump_sender,
            src_to_doc_jump_sender,
        }
    }
    pub fn run(mut self) {
        self.compile();
        let Ok(_) = self
            .fs_watcher
            .watch(&self.compiler_driver.world.root, RecursiveMode::Recursive)
        else {
            error!("TypstActor: failed to watch filesystem events");
            panic!();
        };
        loop {
            let mut recompile = false;
            debug!("TypstActor: waiting for message");
            let Some(mail) = self.mailbox.blocking_recv() else {
                info!("TypstActor: no more messages");
                break;
            };
            recompile |= self.need_recompile(&mail);
            self.process_mail(mail);
            // read the queue to empty
            while let Ok(mail) = self.mailbox.try_recv() {
                recompile |= self.need_recompile(&mail);
                self.process_mail(mail);
            }
            if recompile {
                self.compile();
            }
        }
        info!("TypstActor: exiting");
    }

    fn process_mail(&mut self, mail: TypstActorRequest) {
        match &mail {
            TypstActorRequest::DocToSrcJumpResolve(id) => {
                debug!("TypstActor: processing message: {:?}", mail);
                if let Some(info) = self.resolve_doc_to_src_jump(*id) {
                    let _ = self
                        .doc_to_src_jump_sender
                        .send(EditorActorRequest::DocToSrcJump(info));
                }
            }
            TypstActorRequest::SrcToDocJumpResolve(req) => {
                debug!("TypstActor: processing message: {:?}", mail);
                if let Some(info) = self.resolve_src_to_doc_jump(req) {
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
            TypstActorRequest::FilesystemEvent(_e) => {}
        }
    }

    fn compile(&mut self) {
        self.compiler_driver.world.reset();
        if let Some(doc) = self
            .compiler_driver
            .with_compile_diag::<true, _>(Compiler::compile)
        {
            let _ = self.doc_sender.send(Some(Arc::new(doc))); // it is ok to ignore the error here
            let _ = self
                .renderer_sender
                .send(RenderActorRequest::RenderIncremental);
            comemo::evict(30);
        }
    }

    fn need_recompile(&self, mail: &TypstActorRequest) -> bool {
        match mail {
            TypstActorRequest::DocToSrcJumpResolve(_)
            | TypstActorRequest::SrcToDocJumpResolve(_) => false,
            TypstActorRequest::SyncMemoryFiles(_)
            | TypstActorRequest::UpdateMemoryFiles(_)
            | TypstActorRequest::RemoveMemoryFiles(_) => true,
            TypstActorRequest::FilesystemEvent(e) => self.compiler_driver.relevant(e),
        }
    }

    fn update_memory_files(&mut self, files: &MemoryFiles, reset_shadow: bool) {
        if reset_shadow {
            self.compiler_driver.world.reset_shadow();
        }
        for (path, content) in files.files.iter() {
            let path = Path::new(path).to_owned();
            // todo: is it safe to believe that the path is normalized?
            match self.compiler_driver.world.map_shadow(&path, content) {
                Ok(_) => {}
                Err(e) => {
                    error!(
                        "TypstActor: failed to resolve file: {}, error: {e}",
                        path.display()
                    );
                    return;
                }
            };
        }
    }

    fn remove_shadow_files(&mut self, files: &MemoryFilesShort) {
        for path in files.files.iter() {
            let path = Path::new(path);
            // todo: ignoring the error here
            let _ = self.compiler_driver.world.unmap_shadow(path);
        }
    }

    fn resolve_src_to_doc_jump(&self, req: &SrcToDocJumpRequest) -> Option<SrcToDocJumpInfo> {
        let world = &self.compiler_driver.world;
        let relative_path = Path::new(&req.filepath).strip_prefix(&world.root).ok()?;
        let source_id = FileId::new(None, VirtualPath::new(relative_path));
        let source = world.source(source_id).ok()?;
        let cursor = req.to_byte_offset(&source)?;
        let doc = self.doc_sender.borrow().clone()?;
        let jump_pos = jump_from_cursor(&doc.pages, &source, cursor)?;
        Some(SrcToDocJumpInfo {
            page_no: jump_pos.page.into(),
            x: jump_pos.point.x.to_pt(),
            y: jump_pos.point.y.to_pt(),
        })
    }

    fn resolve_doc_to_src_jump(&self, id: u64) -> Option<DocToSrcJumpInfo> {
        let (src_id, span_number) = (id >> 48, id & 0x0000FFFFFFFFFFFF);
        let src_id = FileId::from_raw(src_id as u16);
        if span_number <= 1 {
            return None;
        }
        let span = typst::syntax::Span::new(src_id, span_number)?;
        let source = self.compiler_driver.world.source(src_id).ok()?;
        let range = source.find(span)?.range();
        let filepath = self.compiler_driver.world.path_for_id(src_id).ok()?;
        Some(DocToSrcJumpInfo::from_option(
            filepath.to_string_lossy().to_string(),
            (
                source.byte_to_line(range.start),
                source.byte_to_column(range.start),
            ),
            (
                source.byte_to_line(range.end),
                source.byte_to_column(range.end),
            ),
        ))
    }
}
