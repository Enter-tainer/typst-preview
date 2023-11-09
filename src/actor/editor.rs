use futures::{SinkExt, StreamExt};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

use crate::{
    actor::outline::Outline, actor::typst::TypstActorRequest, ChangeCursorPositionRequest,
    DocToSrcJumpInfo, MemoryFiles, MemoryFilesShort, SrcToDocJumpRequest,
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum CompileStatus {
    Compiling,
    CompileSuccess,
    CompileError,
}

#[derive(Debug)]
pub enum EditorActorRequest {
    DocToSrcJump(DocToSrcJumpInfo),
    Outline(Outline),
    CompileStatus(CompileStatus),
}

pub struct EditorActor {
    mailbox: mpsc::UnboundedReceiver<EditorActorRequest>,
    editor_websocket_conn: WebSocketStream<TcpStream>,

    world_sender: mpsc::UnboundedSender<TypstActorRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "event")]
enum ControlPlaneMessage {
    #[serde(rename = "changeCursorPosition")]
    ChangeCursorPosition(ChangeCursorPositionRequest),
    #[serde(rename = "panelScrollTo")]
    SrcToDocJump(SrcToDocJumpRequest),
    #[serde(rename = "syncMemoryFiles")]
    SyncMemoryFiles(MemoryFiles),
    #[serde(rename = "updateMemoryFiles")]
    UpdateMemoryFiles(MemoryFiles),
    #[serde(rename = "removeMemoryFiles")]
    RemoveMemoryFiles(MemoryFilesShort),
}

#[derive(Debug, Serialize)]
#[serde(tag = "event")]
enum ControlPlaneResponse {
    #[serde(rename = "editorScrollTo")]
    EditorScrollTo(DocToSrcJumpInfo),
    #[serde(rename = "syncEditorChanges")]
    SyncEditorChanges(()),
    #[serde(rename = "compileStatus")]
    CompileStatus(CompileStatus),
    #[serde(rename = "outline")]
    Outline(Outline),
}

impl EditorActor {
    pub fn new(
        mailbox: mpsc::UnboundedReceiver<EditorActorRequest>,
        editor_websocket_conn: WebSocketStream<TcpStream>,
        world_sender: mpsc::UnboundedSender<TypstActorRequest>,
    ) -> Self {
        Self {
            mailbox,
            editor_websocket_conn,
            world_sender,
        }
    }

    pub async fn run(mut self) {
        self.editor_websocket_conn
            .send(Message::Text(
                serde_json::to_string(&ControlPlaneResponse::SyncEditorChanges(())).unwrap(),
            ))
            .await
            .unwrap();
        loop {
            tokio::select! {
                Some(msg) = self.mailbox.recv() => {
                    debug!("EditorActor: received message from mailbox: {:?}", msg);
                    match msg {
                        EditorActorRequest::DocToSrcJump(jump_info) => {
                            let Ok(_) = self.editor_websocket_conn.send(Message::Text(
                                serde_json::to_string(&ControlPlaneResponse::EditorScrollTo(jump_info)).unwrap(),
                            )).await else {
                                warn!("EditorActor: failed to send DocToSrcJump message to editor");
                                break;
                            };
                        },
                        EditorActorRequest::CompileStatus(status) => {
                            let Ok(_) = self.editor_websocket_conn.send(Message::Text(
                                serde_json::to_string(&ControlPlaneResponse::CompileStatus(status)).unwrap(),
                            )).await else {
                                warn!("EditorActor: failed to send CompileStatus message to editor");
                                break;
                            };
                        },
                        EditorActorRequest::Outline(outline) => {
                            let Ok(_) = self.editor_websocket_conn.send(Message::Text(
                                serde_json::to_string(&ControlPlaneResponse::Outline(outline)).unwrap(),
                            )).await else {
                                warn!("EditorActor: failed to send Outline message to editor");
                                break;
                            };
                        }
                    }
                }
                Some(Ok(Message::Text(msg))) = self.editor_websocket_conn.next() => {
                    let Ok(msg) = serde_json::from_str::<ControlPlaneMessage>(&msg) else {
                        warn!("failed to parse jump request: {:?}", msg);
                        continue;
                    };
                    match msg {
                        ControlPlaneMessage::ChangeCursorPosition(cursor_info) => {
                            debug!("EditorActor: received message from editor: {:?}", cursor_info);
                            self.world_sender.send(TypstActorRequest::ChangeCursorPosition(cursor_info))
                        }
                        ControlPlaneMessage::SrcToDocJump(jump_info) => {
                            debug!("EditorActor: received message from editor: {:?}", jump_info);
                            self.world_sender.send(TypstActorRequest::SrcToDocJumpResolve(jump_info))
                        }
                        ControlPlaneMessage::SyncMemoryFiles(memory_files) => {
                            debug!("EditorActor: received message from editor: SyncMemoryFiles {:?}", memory_files.files.keys().collect::<Vec<_>>());
                            self.world_sender.send(TypstActorRequest::SyncMemoryFiles(memory_files))
                        }
                        ControlPlaneMessage::UpdateMemoryFiles(memory_files) => {
                            debug!("EditorActor: received message from editor: UpdateMemoryFiles {:?}", memory_files.files.keys().collect::<Vec<_>>());
                            self.world_sender.send(TypstActorRequest::UpdateMemoryFiles(memory_files))
                        }
                        ControlPlaneMessage::RemoveMemoryFiles(memory_files) => {
                            debug!("EditorActor: received message from editor: RemoveMemoryFiles {:?}", &memory_files.files);
                            self.world_sender.send(TypstActorRequest::RemoveMemoryFiles(memory_files))
                        }
                    }.unwrap();
                }
            }
        }
        info!("EditorActor: ws disconnected, shutting down whole program");
        std::process::exit(0);
    }
}
