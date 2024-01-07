mod actor;
mod args;
mod debug_loc;
mod outline;

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use futures::SinkExt;
use log::info;
use serde::Deserialize;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use typst::{layout::Position, syntax::Span};
use typst_ts_compiler::service::CompileDriver;
use typst_ts_core::{error::prelude::ZResult, ImmutStr, TypstDocument as Document};

pub use typst_ts_compiler::service::DocToSrcJumpInfo;

use actor::editor::CompileStatus;
use actor::editor::EditorActor;
use actor::typst::TypstActor;
pub use args::*;

#[derive(Debug, Deserialize)]
pub struct ChangeCursorPositionRequest {
    filepath: PathBuf,
    line: usize,
    /// fixme: character is 0-based, UTF-16 code unit.
    /// We treat it as UTF-8 now.
    character: usize,
}

// JSON.stringify({
// 		'event': 'panelScrollTo',
// 		'filepath': bindDocument.uri.fsPath,
// 		'line': activeEditor.selection.active.line,
// 		'character': activeEditor.selection.active.character,
// 	})
#[derive(Debug, Deserialize)]
pub struct SrcToDocJumpRequest {
    filepath: PathBuf,
    line: usize,
    /// fixme: character is 0-based, UTF-16 code unit.
    /// We treat it as UTF-8 now.
    character: usize,
}

impl SrcToDocJumpRequest {
    pub fn to_byte_offset(&self, src: &typst::syntax::Source) -> Option<usize> {
        src.line_column_to_byte(self.line, self.character)
    }
}

#[derive(Debug, Deserialize)]
pub struct MemoryFiles {
    files: HashMap<PathBuf, String>,
}

#[derive(Debug, Deserialize)]
pub struct MemoryFilesShort {
    files: Vec<PathBuf>,
    // mtime: Option<u64>,
}

/// If this file is not found, please refer to https://enter-tainer.github.io/typst-preview/dev.html to build the frontend.
const HTML: &str = include_str!("../addons/vscode/out/frontend/index.html");

pub struct Previewer {
    frontend_html_factory: Box<dyn Fn(PreviewMode) -> ImmutStr>,
    data_plane_handle: tokio::task::JoinHandle<()>,
    control_plane_handle: tokio::task::JoinHandle<()>,
}

impl Previewer {
    /// Get the HTML for the frontend by a given preview mode
    pub fn frontend_html(&self, mode: PreviewMode) -> ImmutStr {
        (self.frontend_html_factory)(mode)
    }

    /// Join the previewer actors.
    // todo: close the actors
    pub async fn join(self) {
        let _ = tokio::join!(self.data_plane_handle, self.control_plane_handle);
    }
}

pub trait CompileHost {
    fn subscribe_doc(&self) -> Option<tokio::sync::watch::Receiver<Option<Arc<Document>>>> {
        None
    }

    fn subscribe_status(&self) -> Option<tokio::sync::watch::Receiver<CompileStatus>> {
        None
    }

    // todo: sync or async
    /// fixme: character is 0-based, UTF-16 code unit. We treat it as UTF-8 now.
    fn resolve_src_to_doc_jump(
        &mut self,
        _filepath: PathBuf,
        _line: usize,
        _character: usize,
    ) -> ZResult<Option<Position>> {
        Ok(None)
    }

    fn resolve_span(&mut self, _span_id: Span) -> ZResult<Option<DocToSrcJumpInfo>> {
        Ok(None)
    }
}

// todo: replace CompileDriver by CompileHost
pub async fn preview(arguments: PreviewArgs, compiler_driver: CompileDriver) -> Previewer {
    let enable_partial_rendering = arguments.enable_partial_rendering;

    // Create the world that serves sources, fonts and files.
    let actor::typst::Channels {
        typst_mailbox,
        doc_watch,
        renderer_mailbox,
        editor_conn,
        webview_conn: (webview_tx, _),
    } = TypstActor::set_up_channels();
    let typst_actor = TypstActor::new(
        compiler_driver,
        typst_mailbox.1,
        doc_watch.0,
        renderer_mailbox.0.clone(),
        editor_conn.0.clone(),
        webview_tx.clone(),
    );

    tokio::spawn(typst_actor.run());

    let (data_plane_port_tx, data_plane_port_rx) = tokio::sync::oneshot::channel();
    let data_plane_addr = arguments.data_plane_host;
    let data_plane_handle = {
        let typst_tx = typst_mailbox.0.clone();
        let webview_tx = webview_tx.clone();
        let doc_watch_rx = doc_watch.1.clone();
        tokio::spawn(async move {
            // Create the event loop and TCP listener we'll accept connections on.
            let try_socket = TcpListener::bind(&data_plane_addr).await;
            let listener = try_socket.expect("Failed to bind");
            info!(
                "Data plane server listening on: {}",
                listener.local_addr().unwrap()
            );
            let _ = data_plane_port_tx.send(listener.local_addr().unwrap().port());
            while let Ok((stream, _)) = listener.accept().await {
                let webview_tx = webview_tx.clone();
                let webview_rx = webview_tx.subscribe();
                let typst_tx = typst_tx.clone();
                let doc_watch_rx = doc_watch_rx.clone();
                let mut conn = accept_connection(stream).await;
                if enable_partial_rendering {
                    conn.send(Message::Binary("partial-rendering,true".into()))
                        .await
                        .unwrap();
                }
                let actor::webview::Channels { svg } =
                    actor::webview::WebviewActor::set_up_channels();
                let webview_actor = actor::webview::WebviewActor::new(
                    conn,
                    svg.1,
                    webview_tx,
                    webview_rx,
                    typst_tx.clone(),
                    renderer_mailbox.0.clone(),
                );
                tokio::spawn(webview_actor.run());
                let render_actor = actor::render::RenderActor::new(
                    renderer_mailbox.0.subscribe(),
                    doc_watch_rx.clone(),
                    typst_tx,
                    svg.0,
                );
                render_actor.spawn();
                let outline_render_actor = actor::render::OutlineRenderActor::new(
                    renderer_mailbox.0.subscribe(),
                    doc_watch_rx,
                    editor_conn.0.clone(),
                );
                outline_render_actor.spawn();
            }
        })
    };

    let control_plane_addr = arguments.control_plane_host;
    let control_plane_handle = {
        let typst_tx = typst_mailbox.0.clone();
        let editor_rx = editor_conn.1;
        tokio::spawn(async move {
            let try_socket = TcpListener::bind(&control_plane_addr).await;
            let listener = try_socket.expect("Failed to bind");
            info!(
                "Control plane server listening on: {}",
                listener.local_addr().unwrap()
            );
            let (stream, _) = listener.accept().await.unwrap();
            let conn = accept_connection(stream).await;
            let editor_actor = EditorActor::new(editor_rx, conn, typst_tx, webview_tx);
            editor_actor.run().await;
        })
    };
    let data_plane_port = data_plane_port_rx.await.unwrap();
    let html = HTML.replace(
        "ws://127.0.0.1:23625",
        format!("ws://127.0.0.1:{data_plane_port}").as_str(),
    );
    // previewMode
    let frontend_html_factory = Box::new(move |mode| -> ImmutStr {
        let mode = match mode {
            PreviewMode::Document => "Doc",
            PreviewMode::Slide => "Slide",
        };
        html.replace(
            "preview-arg:previewMode:Doc",
            format!("preview-arg:previewMode:{}", mode).as_str(),
        )
        .into()
    });

    Previewer {
        frontend_html_factory,
        data_plane_handle,
        control_plane_handle,
    }
}

async fn accept_connection(stream: TcpStream) -> WebSocketStream<TcpStream> {
    let addr = stream
        .peer_addr()
        .expect("connected streams should have a peer address");
    info!("Peer address: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("Error during the websocket handshake occurred");

    info!("New WebSocket connection: {}", addr);
    ws_stream
}
