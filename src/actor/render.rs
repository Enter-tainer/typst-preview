use std::sync::Arc;

use log::{debug, info};
use tokio::sync::{mpsc, watch};
use typst::doc::Document;
use typst_ts_svg_exporter::IncrSvgDocServer;

use super::outline::Outline;
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
        let mut res = Self {
            mailbox,
            document,
            renderer: IncrSvgDocServer::default(),
            svg_sender,
        };
        res.renderer.set_should_attach_debug_info(true);
        res
    }

    pub fn run(mut self) {
        loop {
            let mut has_full_render = false;
            debug!("RenderActor: waiting for message");
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
            debug!("RenderActor: has_full_render: {}", has_full_render);
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
            comemo::evict(30);
            let Ok(_) = self.svg_sender.send(data) else {
                info!("RenderActor: svg_sender is dropped");
                break;
            };
        }
        info!("RenderActor: exiting")
    }
}

pub struct OutlineRenderActor {
    signal: mpsc::UnboundedReceiver<()>,
    document: watch::Receiver<Option<Arc<Document>>>,
    outline_sender: mpsc::UnboundedSender<Outline>,
}

impl OutlineRenderActor {
    pub fn new(
        signal: mpsc::UnboundedReceiver<()>,
        document: watch::Receiver<Option<Arc<Document>>>,
        outline_sender: mpsc::UnboundedSender<Outline>,
    ) -> Self {
        Self {
            signal,
            document,
            outline_sender,
        }
    }

    pub fn run(mut self) {
        loop {
            debug!("OutlineRenderActor: waiting for message");
            let Some(_) = self.signal.blocking_recv() else {
                info!("OutlineRenderActor: no more messages");
                break;
            };
            // read the queue to empty
            while self.signal.try_recv().is_ok() {}
            // if a full render is requested, we render the latest document
            // otherwise, we render the incremental changes for only once
            let Some(document) = self.document.borrow().clone() else {
                info!("OutlineRenderActor: document is not ready");
                continue;
            };
            let data = crate::actor::outline::outline(&document);
            comemo::evict(30);
            debug!("OutlineRenderActor: sending outline");
            let Ok(_) = self.outline_sender.send(data) else {
                info!("OutlineRenderActor: outline_sender is dropped");
                break;
            };
        }
        info!("OutlineRenderActor: exiting")
    }
}
