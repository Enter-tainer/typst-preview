mod actor;
mod args;
mod debug_loc;
mod outline;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use actor::editor::CompileStatus;
use args::PreviewArgs;
use clap::Parser;
use futures::SinkExt;
use hyper::http::Error;
use hyper::service::{make_service_fn, service_fn};
use log::{error, info};
use serde::Deserialize;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use typst_ts_compiler::service::CompileDriver;
use typst_ts_compiler::TypstSystemWorld;
use typst_ts_core::config::CompileOpts;
use typst_ts_core::{ImmutStr, TypstDocument as Document};

use crate::actor::editor::EditorActor;
use crate::actor::typst::CompileClient;
use crate::actor::typst::TypstActor;
use crate::args::{CliArguments, PreviewMode};

pub use typst_ts_compiler::service::DocToSrcJumpInfo;

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

/// Entry point.
#[tokio::main]
async fn main() {
    let _ = env_logger::builder()
        // TODO: set this back to Info
        .filter_module("typst_preview", log::LevelFilter::Debug)
        .filter_module("typst_ts", log::LevelFilter::Info)
        // TODO: set this back to Info
        .filter_module(
            "typst_ts_compiler::service::compile",
            log::LevelFilter::Debug,
        )
        .filter_module("typst_ts_compiler::service::watch", log::LevelFilter::Debug)
        .try_init();
    let arguments = CliArguments::parse();
    info!("Arguments: {:#?}", arguments);
    let entry = if arguments.input.is_absolute() {
        arguments.input.clone()
    } else {
        std::env::current_dir().unwrap().join(&arguments.input)
    };
    let root = if let Some(root) = &arguments.root {
        if root.is_absolute() {
            root.clone()
        } else {
            std::env::current_dir().unwrap().join(root)
        }
    } else {
        std::env::current_dir().unwrap()
    };
    if !entry.starts_with(&root) {
        error!("entry file must be in the root directory");
        std::process::exit(1);
    }

    let compiler_driver = {
        let world = TypstSystemWorld::new(CompileOpts {
            root_dir: root.clone(),
            font_paths: arguments.font_paths.clone(),
            with_embedded_fonts: EMBEDDED_FONT.to_owned(),
            ..CompileOpts::default()
        })
        .expect("incorrect options");

        CompileDriver::new(world).with_entry_file(entry)
    };

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        info!("Ctrl-C received, exiting");
        std::process::exit(0);
    });

    let previewer = preview(arguments.preview, compiler_driver).await;

    previewer.join().await;
}

pub struct Previewer {
    frontend_html: ImmutStr,
    frontend_html_factory: Box<dyn Fn(PreviewMode) -> ImmutStr>,
    data_plane_handle: tokio::task::JoinHandle<()>,
    control_plane_handle: tokio::task::JoinHandle<()>,
}

impl Previewer {
    /// Get the HTML for the frontend (with set preview mode)
    pub fn frontend_html(&self) -> ImmutStr {
        self.frontend_html.clone()
    }

    /// Get the HTML for the frontend by a given preview mode
    pub fn frontend_html_in_mode(&self, mode: PreviewMode) -> ImmutStr {
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

    // todo: design this compile host based on CompileClient
    fn make(self) -> CompileClient;
}

// todo: replace compile driver with compile host
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
                    typst_tx,
                    renderer_mailbox.0.clone(),
                );
                tokio::spawn(webview_actor.run());
                let render_actor = actor::render::RenderActor::new(
                    renderer_mailbox.0.subscribe(),
                    doc_watch_rx.clone(),
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
    let mode = arguments.preview_mode;
    let frontend_html = frontend_html_factory(mode);
    let make_service = make_service_fn(|_| {
        let html = frontend_html.clone();
        async move {
            Ok::<_, hyper::http::Error>(service_fn(move |req| {
                // todo: clone may not be necessary
                let html = html.as_ref().to_owned();
                async move {
                    if req.uri().path() == "/" {
                        log::info!("Serve frontend: {:?}", mode);
                        Ok::<_, Error>(hyper::Response::new(hyper::Body::from(html)))
                    } else {
                        // jump to /
                        let mut res = hyper::Response::new(hyper::Body::empty());
                        *res.status_mut() = hyper::StatusCode::FOUND;
                        res.headers_mut().insert(
                            hyper::header::LOCATION,
                            hyper::header::HeaderValue::from_static("/"),
                        );
                        Ok(res)
                    }
                }
            }))
        }
    });
    let static_file_addr = arguments.static_file_host;
    if !static_file_addr.is_empty() {
        let server = hyper::Server::bind(&static_file_addr.parse().unwrap()).serve(make_service);
        if !arguments.dont_open_in_browser {
            if let Err(e) = open::that_detached(format!("http://{}", server.local_addr())) {
                error!("failed to open browser: {}", e);
            };
        }
        info!("Static file server listening on: {}", server.local_addr());
        if let Err(e) = server.await {
            error!("Static file server error: {}", e);
        }
    }

    Previewer {
        frontend_html,
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

use std::borrow::Cow;

pub static EMBEDDED_FONT: &[Cow<'_, [u8]>] = &[
    // Embed default fonts.
    Cow::Borrowed(include_bytes!("../assets/fonts/LinLibertine_R.ttf").as_slice()),
    Cow::Borrowed(include_bytes!("../assets/fonts/LinLibertine_RB.ttf").as_slice()),
    Cow::Borrowed(include_bytes!("../assets/fonts/LinLibertine_RBI.ttf").as_slice()),
    Cow::Borrowed(include_bytes!("../assets/fonts/LinLibertine_RI.ttf").as_slice()),
    Cow::Borrowed(include_bytes!("../assets/fonts/NewCMMath-Book.otf").as_slice()),
    Cow::Borrowed(include_bytes!("../assets/fonts/NewCMMath-Regular.otf").as_slice()),
    Cow::Borrowed(include_bytes!("../assets/fonts/NewCM10-Regular.otf").as_slice()),
    Cow::Borrowed(include_bytes!("../assets/fonts/NewCM10-Bold.otf").as_slice()),
    Cow::Borrowed(include_bytes!("../assets/fonts/NewCM10-Italic.otf").as_slice()),
    Cow::Borrowed(include_bytes!("../assets/fonts/NewCM10-BoldItalic.otf").as_slice()),
    Cow::Borrowed(include_bytes!("../assets/fonts/DejaVuSansMono.ttf").as_slice()),
    Cow::Borrowed(include_bytes!("../assets/fonts/DejaVuSansMono-Bold.ttf").as_slice()),
    Cow::Borrowed(include_bytes!("../assets/fonts/DejaVuSansMono-Oblique.ttf").as_slice()),
    Cow::Borrowed(include_bytes!("../assets/fonts/DejaVuSansMono-BoldOblique.ttf").as_slice()),
    // Embed CJK fonts.
    #[cfg(feature = "embedded-cjk-fonts")]
    Cow::Borrowed(include_bytes!("../assets/fonts/InriaSerif-Bold.ttf").as_slice()),
    #[cfg(feature = "embedded-cjk-fonts")]
    Cow::Borrowed(include_bytes!("../assets/fonts/InriaSerif-BoldItalic.ttf").as_slice()),
    #[cfg(feature = "embedded-cjk-fonts")]
    Cow::Borrowed(include_bytes!("../assets/fonts/InriaSerif-Italic.ttf").as_slice()),
    #[cfg(feature = "embedded-cjk-fonts")]
    Cow::Borrowed(include_bytes!("../assets/fonts/InriaSerif-Regular.ttf").as_slice()),
    #[cfg(feature = "embedded-cjk-fonts")]
    Cow::Borrowed(include_bytes!("../assets/fonts/Roboto-Regular.ttf").as_slice()),
    #[cfg(feature = "embedded-cjk-fonts")]
    Cow::Borrowed(include_bytes!("../assets/fonts/NotoSerifCJKsc-Regular.otf").as_slice()),
    // Embed emoji fonts.
    #[cfg(feature = "embedded-emoji-fonts")]
    Cow::Borrowed(include_bytes!("../assets/fonts/TwitterColorEmoji.ttf").as_slice()),
    #[cfg(feature = "embedded-emoji-fonts")]
    Cow::Borrowed(include_bytes!("../assets/fonts/NotoColorEmoji.ttf").as_slice()),
];
