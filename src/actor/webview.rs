use futures::{SinkExt, StreamExt};
use log::{debug, info};
use tokio::{
    net::TcpStream,
    sync::{broadcast, mpsc},
};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};
use typst_ts_core::vector::span_id_from_u64;

use super::{render::RenderActorRequest, typst::TypstActorRequest};
use crate::{debug_loc::DocumentPosition, outline::Outline};

pub type CursorPosition = DocumentPosition;
pub type SrcToDocJumpInfo = DocumentPosition;

#[derive(Debug, Clone)]
pub enum WebviewActorRequest {
    ViewportPosition(DocumentPosition),
    SrcToDocJump(SrcToDocJumpInfo),
    CursorPosition(CursorPosition),
}

fn position_req(
    event: &'static str,
    DocumentPosition { page_no, x, y }: DocumentPosition,
) -> String {
    format!("{event},{page_no} {x} {y}")
}

pub struct WebviewActor {
    webview_websocket_conn: WebSocketStream<TcpStream>,
    svg_receiver: mpsc::UnboundedReceiver<Vec<u8>>,
    mailbox: broadcast::Receiver<WebviewActorRequest>,

    doc_to_src_sender: mpsc::UnboundedSender<TypstActorRequest>,
    render_full_latest_sender: mpsc::UnboundedSender<RenderActorRequest>,
    render_full_latest_outline_sender: mpsc::UnboundedSender<()>,
}

pub struct Channels {
    pub svg: (
        mpsc::UnboundedSender<Vec<u8>>,
        mpsc::UnboundedReceiver<Vec<u8>>,
    ),
    pub outline: (
        mpsc::UnboundedSender<Outline>,
        mpsc::UnboundedReceiver<Outline>,
    ),
    pub render_full: (
        mpsc::UnboundedSender<RenderActorRequest>,
        mpsc::UnboundedReceiver<RenderActorRequest>,
    ),
    pub render_outline: (mpsc::UnboundedSender<()>, mpsc::UnboundedReceiver<()>),
}

impl WebviewActor {
    pub fn set_up_channels() -> Channels {
        Channels {
            svg: mpsc::unbounded_channel(),
            outline: mpsc::unbounded_channel(),
            render_full: mpsc::unbounded_channel(),
            render_outline: mpsc::unbounded_channel(),
        }
    }
    pub fn new(
        websocket_conn: WebSocketStream<TcpStream>,
        svg_receiver: mpsc::UnboundedReceiver<Vec<u8>>,
        mailbox: broadcast::Receiver<WebviewActorRequest>,
        doc_to_src_sender: mpsc::UnboundedSender<TypstActorRequest>,
        render_full_latest_sender: mpsc::UnboundedSender<RenderActorRequest>,
        render_full_latest_outline_sender: mpsc::UnboundedSender<()>,
    ) -> Self {
        Self {
            webview_websocket_conn: websocket_conn,
            svg_receiver,
            mailbox,
            doc_to_src_sender,
            render_full_latest_sender,
            render_full_latest_outline_sender,
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                Ok(msg) = self.mailbox.recv() => {
                    debug!("WebviewActor: received message from mailbox: {:?}", msg);
                    match msg {
                        WebviewActorRequest::SrcToDocJump(jump_info) => {
                            let msg = position_req("jump", jump_info);
                            self.webview_websocket_conn.send(Message::Binary(msg.into_bytes())).await.unwrap();
                        }
                        WebviewActorRequest::ViewportPosition(jump_info) => {
                            let msg = position_req("viewport", jump_info);
                            self.webview_websocket_conn.send(Message::Binary(msg.into_bytes())).await.unwrap();
                        }
                        WebviewActorRequest::CursorPosition(jump_info) => {
                            let msg = position_req("cursor", jump_info);
                            self.webview_websocket_conn.send(Message::Binary(msg.into_bytes())).await.unwrap();
                        }
                    }
                }
                Some(svg) = self.svg_receiver.recv() => {
                    debug!("WebviewActor: received svg from renderer");
                    self.webview_websocket_conn.send(Message::Binary(svg)).await.unwrap();
                }
                Some(msg) = self.webview_websocket_conn.next() => {
                    debug!("WebviewActor: received message from websocket: {:?}", msg);
                    let Ok(msg) = msg else {
                        info!("WebviewActor: no more messages from websocket: {}", msg.unwrap_err());
                      break;
                    };
                    let Message::Text(msg) = msg else {
                        info!("WebviewActor: received non-text message from websocket: {:?}", msg);
                        let _ = self.webview_websocket_conn.send(Message::Text(format!("Webview Actor: error, received non-text message: {}", msg))).await;
                        break;
                    };
                    if msg == "current" {
                        self.render_full_latest_sender.send(RenderActorRequest::RenderFullLatest).unwrap();
                        self.render_full_latest_outline_sender.send(()).unwrap();
                    } else if msg.starts_with("srclocation") {
                        let location = msg.split(' ').nth(1).unwrap();
                        let id = u64::from_str_radix(location, 16).unwrap();
                        if let Some(span) = span_id_from_u64(id) {
                            let Ok(_) = self.doc_to_src_sender.send(TypstActorRequest::DocToSrcJumpResolve(span)) else {
                                info!("WebviewActor: failed to send DocToSrcJumpResolve message to TypstActor");
                                break;
                            };
                        };
                    } else {
                        info!("WebviewActor: received unknown message from websocket: {}", msg);
                        self.webview_websocket_conn.send(Message::Text(format!("error, received unknown message: {}", msg))).await.unwrap();
                        break;
                    }
                }
                else => {
                    break;
                }
            }
        }
        info!("WebviewActor: exiting");
    }
}
