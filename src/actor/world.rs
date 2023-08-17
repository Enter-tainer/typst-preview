use std::{path::Path, sync::Arc};

use crate::{
    jump_from_cursor, DocToSrcJumpInfo, MemoryFiles, MemoryFilesShort, SrcToDocJumpRequest,
};
use log::{error, info};
use tokio::sync::{mpsc, watch};
use typst::syntax::FileId;
use typst::{doc::Document, World};
use typst_ts_compiler::service::CompileDriver;

use super::{
    editor::EditorActorRequest,
    webview::{SrcToDocJumpInfo, WebviewActorRequest},
};

pub enum WorldActorRequest {
    DocToSrcJumpResolve(u64),
    SrcToDocJumpResolve(SrcToDocJumpRequest),

    SyncMemoryFiles(MemoryFiles),
    UpdateMemoryFiles(MemoryFiles),
    RemoveMemoryFiles(MemoryFilesShort),

    FilesystemEvent(notify::Event),
}

pub struct WorldActor {
    world: CompileDriver,

    mailbox: mpsc::UnboundedReceiver<WorldActorRequest>,
    doc_sender: watch::Sender<Arc<Document>>,
    doc_to_src_jump_sender: mpsc::UnboundedSender<EditorActorRequest>,
    src_to_doc_jump_sender: mpsc::UnboundedSender<WebviewActorRequest>,
}

impl WorldActor {
    pub fn new(
        world: CompileDriver,
        mailbox: mpsc::UnboundedReceiver<WorldActorRequest>,
        doc_sender: watch::Sender<Arc<Document>>,
        doc_to_src_jump_sender: mpsc::UnboundedSender<EditorActorRequest>,
        src_to_doc_jump_sender: mpsc::UnboundedSender<WebviewActorRequest>,
    ) -> Self {
        Self {
            world,
            mailbox,
            doc_sender,
            doc_to_src_jump_sender,
            src_to_doc_jump_sender,
        }
    }
    pub fn run(mut self) {
        loop {
            let mut recompile = false;
            let Some(mail) = self.mailbox.blocking_recv() else {
                info!("WorldActor: no more messages");
                break;
            };
            recompile |= self.need_recompile(&mail);
            // read the queue to empty
            while let Ok(mail) = self.mailbox.try_recv() {
                recompile |= self.need_recompile(&mail);
                match &mail {
                    WorldActorRequest::DocToSrcJumpResolve(id) => {
                        if let Some(info) = self.resolve_doc_to_src_jump(*id) {
                            let _ = self
                                .doc_to_src_jump_sender
                                .send(EditorActorRequest::DocToSrcJump(info));
                        }
                    }
                    WorldActorRequest::SrcToDocJumpResolve(req) => {
                        if let Some(info) = self.resolve_src_to_doc_jump(req) {
                            let _ = self
                                .src_to_doc_jump_sender
                                .send(WebviewActorRequest::SrcToDocJump(info));
                        }
                    }
                    WorldActorRequest::SyncMemoryFiles(m) => {
                        self.update_memory_files(m, true);
                    }
                    WorldActorRequest::UpdateMemoryFiles(m) => {
                        self.update_memory_files(m, false);
                    }
                    WorldActorRequest::RemoveMemoryFiles(m) => {
                        self.remove_shadow_files(m);
                    }
                    WorldActorRequest::FilesystemEvent(_e) => {}
                }
            }
            if recompile {
                self.world.world.reset();
                if let Some(doc) = self
                    .world
                    .with_compile_diag::<true, _>(CompileDriver::compile)
                {
                    let _ = self.doc_sender.send(Arc::new(doc)); // it is ok to ignore the error here
                    comemo::evict(30);
                }
            }
        }
        info!("WorldActor: exiting");
    }

    fn need_recompile(&self, mail: &WorldActorRequest) -> bool {
        match mail {
            WorldActorRequest::DocToSrcJumpResolve(_)
            | WorldActorRequest::SrcToDocJumpResolve(_) => false,
            WorldActorRequest::SyncMemoryFiles(_)
            | WorldActorRequest::UpdateMemoryFiles(_)
            | WorldActorRequest::RemoveMemoryFiles(_) => true,
            WorldActorRequest::FilesystemEvent(e) => self.world.relevant(e),
        }
    }

    fn update_memory_files(&mut self, files: &MemoryFiles, reset_shadow: bool) {
        if reset_shadow {
            self.world.world.reset_shadow();
        }
        for (path, content) in files.files.iter() {
            let path = Path::new(path).to_owned();
            let id = self.world.id_for_path(path.clone());
            let Ok(_) = self.world.world.resolve_with(&path, id, content) else {
                error!("WorldActor: failed to resolve file: {}", path.display());
                return;
            };
        }
    }

    fn remove_shadow_files(&mut self, files: &MemoryFilesShort) {
        for path in files.files.iter() {
            let path = Path::new(path);
            self.world.world.remove_shadow(path);
        }
    }

    fn resolve_src_to_doc_jump(&self, req: &SrcToDocJumpRequest) -> Option<SrcToDocJumpInfo> {
        let world = &self.world.world;
        let relative_path = Path::new(&req.filepath).strip_prefix(&world.root).ok()?;
        let source_id = FileId::new(None, &Path::new("/").join(relative_path));
        let source = world.source(source_id).ok()?;
        let cursor = req.to_byte_offset(&source)?;
        let doc = self.doc_sender.borrow();
        let jump_pos = jump_from_cursor(&doc.pages, &source, cursor)?;
        Some(SrcToDocJumpInfo {
            page_no: jump_pos.page.into(),
            x: jump_pos.point.x.to_pt(),
            y: jump_pos.point.y.to_pt(),
        })
    }

    fn resolve_doc_to_src_jump(&self, id: u64) -> Option<DocToSrcJumpInfo> {
        let (src_id, span_number) = (id >> 48, id & 0x0000FFFFFFFFFFFF);
        let src_id = FileId::from_u16(src_id as u16);
        if src_id == FileId::detached() || span_number <= 1 {
            return None;
        }
        let span = typst::syntax::Span::new(src_id, span_number);
        let source = self.world.world.source(src_id).ok()?;
        let range = source.find(span)?.range();
        let filepath = self.world.world.path_for_id(src_id).ok()?;
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
