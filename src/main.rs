mod actor;
mod args;
use clap::Parser;

use futures::SinkExt;
use hyper::http::Error;
use hyper::service::{make_service_fn, service_fn};
use log::{error, info};

use serde::Deserialize;
use std::collections::HashMap;
use tokio_tungstenite::tungstenite::Message;

use std::path::PathBuf;

use tokio::net::{TcpListener, TcpStream};

use crate::actor::editor::EditorActor;
use crate::actor::typst::TypstActor;
use crate::args::{CliArguments, PreviewMode};

use tokio_tungstenite::WebSocketStream;

use typst_ts_compiler::service::CompileDriver;
use typst_ts_compiler::TypstSystemWorld;
use typst_ts_core::config::CompileOpts;

#[allow(dead_code)]
/// A summary of the input arguments relevant to compilation.
struct CompileSettings {
    /// The path to the input file.
    input: PathBuf,

    /// Whether to watch the input files for changes.
    watch: bool,

    /// The root directory for absolute paths.
    root: Option<PathBuf>,

    /// The paths to search for fonts.
    font_paths: Vec<PathBuf>,
}

impl CompileSettings {
    /// Create a new compile settings from the field values.
    pub fn new(
        input: PathBuf,
        watch: bool,
        root: Option<PathBuf>,
        font_paths: Vec<PathBuf>,
    ) -> Self {
        Self {
            input,
            watch,
            root,
            font_paths,
        }
    }

    /// Create a new compile settings from the CLI arguments and a compile command.
    ///
    /// # Panics
    /// Panics if the command is not a compile or watch command.
    pub fn with_arguments(args: CliArguments) -> Self {
        Self::new(args.input, true, args.root, args.font_paths)
    }
}

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
    let command = CompileSettings::with_arguments(arguments.clone());
    let enable_partial_rendering = arguments.enable_partial_rendering;
    let entry = if command.input.is_absolute() {
        command.input.clone()
    } else {
        std::env::current_dir().unwrap().join(command.input)
    };
    let root = if let Some(root) = &command.root {
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

    // Create the world that serves sources, fonts and files.
    let actor::typst::Channels {
        typst_mailbox,
        doc_watch,
        renderer_mailbox,
        editor_conn,
        webview_conn,
    } = TypstActor::set_up_channels();
    let typst_actor = TypstActor::new(
        compiler_driver,
        typst_mailbox.1,
        doc_watch.0,
        renderer_mailbox.0.clone(),
        editor_conn.0.clone(),
        webview_conn.0.clone(),
    );

    tokio::spawn(typst_actor.run());

    let (data_plane_port_tx, data_plane_port_rx) = tokio::sync::oneshot::channel();
    let data_plane_addr = arguments.data_plane_host;
    let data_plane_handle = {
        let typst_tx = typst_mailbox.0.clone();
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
                let src_to_doc_rx = webview_conn.0.subscribe();
                let typst_tx = typst_tx.clone();
                let doc_watch_rx = doc_watch_rx.clone();
                let mut conn = accept_connection(stream).await;
                if enable_partial_rendering {
                    conn.send(Message::Binary("partial-rendering,true".into()))
                        .await
                        .unwrap();
                }
                let actor::webview::Channels {
                    svg,
                    mut outline,
                    render_full,
                    render_outline,
                } = actor::webview::WebviewActor::set_up_channels();
                let render_tx = render_full.0.clone();
                let render_outline_tx = render_outline.0.clone();
                let webview_actor = actor::webview::WebviewActor::new(
                    conn,
                    svg.1,
                    src_to_doc_rx,
                    typst_tx,
                    render_full.0,
                    render_outline_tx,
                );
                tokio::spawn(async move {
                    webview_actor.run().await;
                });
                let render_actor =
                    actor::render::RenderActor::new(render_full.1, doc_watch_rx.clone(), svg.0);
                std::thread::spawn(move || {
                    render_actor.run();
                });
                let outline_render_actor = actor::render::OutlineRenderActor::new(
                    render_outline.1,
                    doc_watch_rx,
                    outline.0,
                );
                std::thread::spawn(move || {
                    outline_render_actor.run();
                });
                let mut renderer_rx = renderer_mailbox.0.subscribe();
                let renderer_outline_rx = render_outline.0;
                tokio::spawn(async move {
                    while let Ok(msg) = renderer_rx.recv().await {
                        let Ok(_) = render_tx.send(msg) else {
                            break;
                        };
                        let Ok(_) = renderer_outline_rx.send(()) else {
                            break;
                        };
                    }
                });

                let editor_tx = editor_conn.0.clone();
                tokio::spawn(async move {
                    while let Some(msg) = outline.1.recv().await {
                        editor_tx
                            .send(actor::editor::EditorActorRequest::Outline(msg))
                            .unwrap();
                    }
                });
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
            let editor_actor = EditorActor::new(editor_rx, conn, typst_tx);
            editor_actor.run().await;
        })
    };
    let static_file_addr = arguments.static_file_host;
    let data_plane_port = data_plane_port_rx.await.unwrap();
    let make_service = make_service_fn(|_| {
        async move {
            Ok::<_, hyper::http::Error>(service_fn(move |req| {
                async move {
                    if req.uri().path() == "/" {
                        let html = HTML.replace(
                            "ws://127.0.0.1:23625",
                            format!("ws://127.0.0.1:{data_plane_port}").as_str(),
                        );
                        // previewMode
                        let mode = match arguments.preview_mode {
                            PreviewMode::Document => "Doc",
                            PreviewMode::Slide => "Slide",
                        };
                        let html = html.replace(
                            "preview-arg:previewMode:Doc",
                            format!("preview-arg:previewMode:{}", mode).as_str(),
                        );
                        log::info!("Preview mode: {}", mode);
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
    let _ = tokio::join!(data_plane_handle, control_plane_handle);
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
