use std::sync::Arc;

use log::{debug, info, trace};
use serde::Deserialize;
use tokio::sync::{broadcast, mpsc, watch};
use typst::model::Document;
use typst_ts_svg_exporter::IncrSvgDocServer;

use super::{editor::EditorActorRequest, typst::TypstActorRequest};

#[derive(Debug, Clone, Deserialize)]
pub struct ResolveSpanRequest(Vec<(u32, u32, String)>);

#[derive(Debug, Clone)]
pub enum RenderActorRequest {
    RenderFullLatest,
    RenderIncremental,
    ResolveSpan(ResolveSpanRequest),
}

impl RenderActorRequest {
    pub fn is_full_render(&self) -> bool {
        match self {
            Self::RenderFullLatest => true,
            Self::RenderIncremental => false,
            Self::ResolveSpan(_) => false,
        }
    }
}

pub struct RenderActor {
    mailbox: broadcast::Receiver<RenderActorRequest>,
    document: watch::Receiver<Option<Arc<Document>>>,
    renderer: IncrSvgDocServer,
    resolve_sender: mpsc::UnboundedSender<TypstActorRequest>,
    svg_sender: mpsc::UnboundedSender<Vec<u8>>,
}

impl RenderActor {
    pub fn new(
        mailbox: broadcast::Receiver<RenderActorRequest>,
        document: watch::Receiver<Option<Arc<Document>>>,
        resolve_sender: mpsc::UnboundedSender<TypstActorRequest>,
        svg_sender: mpsc::UnboundedSender<Vec<u8>>,
    ) -> Self {
        let mut res = Self {
            mailbox,
            document,
            renderer: IncrSvgDocServer::default(),
            resolve_sender,
            svg_sender,
        };
        res.renderer.set_should_attach_debug_info(true);
        res
    }

    pub fn spawn(self) {
        std::thread::Builder::new()
            .name("RenderActor".to_owned())
            .spawn(move || self.run())
            .unwrap();
    }

    async fn process_message(&mut self, msg: RenderActorRequest) -> bool {
        trace!("RenderActor: received message: {:?}", msg);

        let res = msg.is_full_render();

        if let RenderActorRequest::ResolveSpan(ResolveSpanRequest(spans)) = msg {
            info!("RenderActor: resolving span: {:?}", spans);
            let spans = match self.renderer.source_span(&spans) {
                Ok(spans) => spans,
                Err(e) => {
                    info!("RenderActor: failed to resolve span: {}", e);
                    return false;
                }
            };

            info!("RenderActor: resolved span: {:?}", spans);
            // end position is used
            if let Some(spans) = spans {
                let Ok(_) = self
                    .resolve_sender
                    .send(TypstActorRequest::DocToSrcJumpResolve(spans))
                else {
                    info!("RenderActor: resolve_sender is dropped");
                    return false;
                };
            }
        }

        res
    }

    #[tokio::main(flavor = "current_thread")]
    async fn run(mut self) {
        loop {
            let mut has_full_render = false;
            debug!("RenderActor: waiting for message");
            match self.mailbox.recv().await {
                Ok(msg) => {
                    has_full_render |= self.process_message(msg).await;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("RenderActor: no more messages");
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    info!("RenderActor: lagged message. Some events are dropped");
                }
            }
            // read the queue to empty
            while let Ok(msg) = self.mailbox.try_recv() {
                has_full_render |= self.process_message(msg).await;
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
    signal: broadcast::Receiver<RenderActorRequest>,
    document: watch::Receiver<Option<Arc<Document>>>,
    editor_tx: mpsc::UnboundedSender<EditorActorRequest>,
}

impl OutlineRenderActor {
    pub fn new(
        signal: broadcast::Receiver<RenderActorRequest>,
        document: watch::Receiver<Option<Arc<Document>>>,
        editor_tx: mpsc::UnboundedSender<EditorActorRequest>,
    ) -> Self {
        Self {
            signal,
            document,
            editor_tx,
        }
    }

    pub fn spawn(self) {
        std::thread::Builder::new()
            .name("OutlineRenderActor".to_owned())
            .spawn(move || self.run())
            .unwrap();
    }

    #[tokio::main(flavor = "current_thread")]
    async fn run(mut self) {
        loop {
            debug!("OutlineRenderActor: waiting for message");
            match self.signal.recv().await {
                Ok(msg) => {
                    debug!("OutlineRenderActor: received message: {:?}", msg);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("OutlineRenderActor: no more messages");
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    info!("OutlineRenderActor: lagged message. Some events are dropped");
                }
            }
            // read the queue to empty
            while self.signal.try_recv().is_ok() {}
            // if a full render is requested, we render the latest document
            // otherwise, we render the incremental changes for only once
            let Some(document) = self.document.borrow().clone() else {
                info!("OutlineRenderActor: document is not ready");
                continue;
            };
            let data = crate::outline::outline(&document);
            comemo::evict(30);
            debug!("OutlineRenderActor: sending outline");
            let Ok(_) = self.editor_tx.send(EditorActorRequest::Outline(data)) else {
                info!("OutlineRenderActor: outline_sender is dropped");
                break;
            };
        }
        info!("OutlineRenderActor: exiting")
    }
}
