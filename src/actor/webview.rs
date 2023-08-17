use futures::{SinkExt, StreamExt};
use log::info;
use tokio::{net::TcpStream, sync::mpsc};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

use super::{render::RenderActorRequest, world::WorldActorRequest};

pub struct SrcToDocJumpInfo {
    pub page_no: usize,
    pub x: f64,
    pub y: f64,
}

pub enum WebviewActorRequest {
    SrcToDocJump(SrcToDocJumpInfo),
}

fn src_to_doc_jump_to_string(page_no: usize, x: f64, y: f64) -> String {
    format!("jump,{page_no} {x} {y}")
}

pub struct WebviewActor {
    websocket_conn: WebSocketStream<TcpStream>,
    svg_receiver: mpsc::UnboundedReceiver<Vec<u8>>,
    mailbox: mpsc::UnboundedReceiver<WebviewActorRequest>,

    doc_to_src_sender: mpsc::UnboundedSender<WorldActorRequest>,
    render_full_latest_sender: mpsc::UnboundedSender<RenderActorRequest>,
}

impl WebviewActor {
    pub fn new(
        websocket_conn: WebSocketStream<TcpStream>,
        svg_receiver: mpsc::UnboundedReceiver<Vec<u8>>,
        mailbox: mpsc::UnboundedReceiver<WebviewActorRequest>,
        doc_to_src_sender: mpsc::UnboundedSender<WorldActorRequest>,
        render_full_latest_sender: mpsc::UnboundedSender<RenderActorRequest>,
    ) -> Self {
        Self {
            websocket_conn,
            svg_receiver,
            mailbox,
            doc_to_src_sender,
            render_full_latest_sender,
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                Some(msg) = self.mailbox.recv() => {
                    match msg {
                        WebviewActorRequest::SrcToDocJump(jump_info) => {
                            let SrcToDocJumpInfo { page_no, x, y } = jump_info;
                            let msg = src_to_doc_jump_to_string(page_no, x, y);
                            self.websocket_conn.send(Message::Text(msg)).await.unwrap();
                        }
                    }
                }
                Some(svg) = self.svg_receiver.recv() => {
                    self.websocket_conn.send(Message::Binary(svg)).await.unwrap();
                }
                Some(msg) = self.websocket_conn.next() => {
                    let Ok(msg) = msg else {
                        info!("WebviewActor: no more messages from websocket: {}", msg.unwrap_err());
                      break;
                    };
                    let Message::Text(msg) = msg else {
                        info!("WebviewActor: received non-text message from websocket: {:?}", msg);
                        self.websocket_conn.send(Message::Text(format!("error, received non-text message: {}", msg))).await.unwrap();
                        break;
                    };
                    if msg == "current" {
                        self.render_full_latest_sender.send(RenderActorRequest::RenderFullLatest).unwrap();
                    } else if msg.starts_with("src_location") {
                        let location = msg.split(' ').nth(1).unwrap();
                        let id = u64::from_str_radix(location, 16).unwrap();
                        let Ok(_) = self.doc_to_src_sender.send(WorldActorRequest::DocToSrcJumpResolve(id)) else {
                            info!("WebviewActor: failed to send DocToSrcJumpResolve message to WorldActor");
                            break;
                        };
                    } else {
                        info!("WebviewActor: received unknown message from websocket: {}", msg);
                        self.websocket_conn.send(Message::Text(format!("error, received unknown message: {}", msg))).await.unwrap();
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
