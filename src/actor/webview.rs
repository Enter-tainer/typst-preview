use futures::{SinkExt, StreamExt};
use log::{debug, info};
use tokio::{
    net::TcpStream,
    sync::{broadcast, mpsc},
};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

use super::{outline::Outline, render::RenderActorRequest, typst::TypstActorRequest};

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct CursorPosition {
    pub page_no: usize,
    pub x: f64,
    pub y: f64,
}

pub type SrcToDocJumpInfo = CursorPosition;

#[derive(Debug, Clone)]
pub enum WebviewActorRequest {
    SrcToDocJump(SrcToDocJumpInfo),
    CursorPosition(CursorPosition),
}

fn src_to_doc_jump_to_string(page_no: usize, x: f64, y: f64) -> String {
    format!("jump,{page_no} {x} {y}")
}

fn cursor_position_to_string(page_no: usize, x: f64, y: f64) -> String {
    format!("cursor,{page_no} {x} {y}")
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
                            let SrcToDocJumpInfo { page_no, x, y } = jump_info;
                            let msg = src_to_doc_jump_to_string(page_no, x, y);
                            self.webview_websocket_conn.send(Message::Binary(msg.into_bytes())).await.unwrap();
                        }
                        WebviewActorRequest::CursorPosition(jump_info) => {
                            let SrcToDocJumpInfo { page_no, x, y } = jump_info;
                            let msg = cursor_position_to_string(page_no, x, y);
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
                        let Ok(_) = self.doc_to_src_sender.send(TypstActorRequest::DocToSrcJumpResolve(id)) else {
                            info!("WebviewActor: failed to send DocToSrcJumpResolve message to TypstActor");
                            break;
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
