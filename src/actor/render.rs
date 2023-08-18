use std::sync::Arc;

use log::info;
use tokio::sync::{mpsc, watch};
use typst::doc::Document;
use typst_ts_svg_exporter::IncrSvgDocServer;
#[derive(Debug, Clone, Copy)]
pub enum RenderActorRequest {
    RenderFullLatest,
    RenderIncremental,
}

impl RenderActorRequest {
    pub fn is_full_render(&self) -> bool {
        match self {
            Self::RenderFullLatest => true,
            Self::RenderIncremental => false,
        }
    }
}

pub struct RenderActor {
    mailbox: mpsc::UnboundedReceiver<RenderActorRequest>,
    document: watch::Receiver<Option<Arc<Document>>>,
    renderer: IncrSvgDocServer,
    svg_sender: mpsc::UnboundedSender<Vec<u8>>,
}

impl RenderActor {
    pub fn new(
        mailbox: mpsc::UnboundedReceiver<RenderActorRequest>,
        document: watch::Receiver<Option<Arc<Document>>>,
        svg_sender: mpsc::UnboundedSender<Vec<u8>>,
    ) -> Self {
        Self {
            mailbox,
            document,
            renderer: IncrSvgDocServer::default(),
            svg_sender,
        }
    }

    pub fn run(mut self) {
        loop {
            let mut has_full_render = false;
            let Some(msg) = self.mailbox.blocking_recv() else {
                info!("RenderActor: no more messages");
                break;
            };
            has_full_render |= msg.is_full_render();
            // read the queue to empty
            while let Ok(msg) = self.mailbox.try_recv() {
                has_full_render |= msg.is_full_render();
            }
            // if a full render is requested, we render the latest document
            // otherwise, we render the incremental changes for only once
            let has_full_render = has_full_render;
            let Some(document) = self.document.borrow().clone() else {
                info!("RenderActor: document is not ready");
                continue;
            };
            let data = if has_full_render {
                if let Some(data) = self.renderer.pack_current() {
                    data
                } else {
                    self.renderer.pack_delta(document)
                }
            } else {
                self.renderer.pack_delta(document)
            };
            let Ok(_) = self.svg_sender.send(data) else {
                info!("RenderActor: svg_sender is dropped");
                break;
            };
        }
        info!("RenderActor: exiting")
    }
}
