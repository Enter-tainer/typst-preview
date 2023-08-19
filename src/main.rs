mod actor;
mod args;
use clap::Parser;

use futures::SinkExt;
use hyper::http::Error;
use hyper::service::{make_service_fn, service_fn};
use log::{error, info};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio_tungstenite::tungstenite::Message;

use typst::geom::Point;

use std::num::NonZeroUsize;
use std::path::PathBuf;

use tokio::net::{TcpListener, TcpStream};
use typst::doc::{Frame, FrameItem, Position};

use crate::actor::editor::EditorActor;
use crate::actor::world::WorldActor;
use crate::args::CliArguments;

use tokio_tungstenite::WebSocketStream;

use typst::syntax::{LinkedNode, Source, Span, SyntaxKind};

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

#[derive(Debug, Serialize)]
pub struct DocToSrcJumpInfo {
    filepath: String,
    start: Option<(usize, usize)>, // row, column
    end: Option<(usize, usize)>,
}

impl DocToSrcJumpInfo {
    pub fn from_option(
        filepath: String,
        start: (Option<usize>, Option<usize>),
        end: (Option<usize>, Option<usize>),
    ) -> Self {
        Self {
            filepath,
            start: start.0.zip(start.1),
            end: end.0.zip(end.1),
        }
    }
}

// JSON.stringify({
// 		'event': 'panelScrollTo',
// 		'filepath': bindDocument.uri.fsPath,
// 		'line': activeEditor.selection.active.line,
// 		'character': activeEditor.selection.active.character,
// 	})
#[derive(Debug, Deserialize)]
pub struct SrcToDocJumpRequest {
    filepath: String,
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
    files: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct MemoryFilesShort {
    files: Vec<String>,
}

const HTML: &str = include_str!("../addons/vscode/out/frontend/index.html");

/// Entry point.
#[tokio::main]
async fn main() {
    let _ = env_logger::builder()
        // TODO: set this back to Info
        .filter_module("typst_preview", log::LevelFilter::Debug)
        .filter_module("typst_ts", log::LevelFilter::Info)
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
    let compile_driver = {
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
    let world = compile_driver;
    let actor::world::Channels {
        world_mailbox,
        doc_watch,
        renderer_mailbox,
        doc_to_src_jump,
        src_to_doc_jump,
    } = WorldActor::set_up_channels();
    let world_actor = WorldActor::new(
        world,
        world_mailbox.1,
        world_mailbox.0.clone(),
        doc_watch.0,
        renderer_mailbox.0.clone(),
        doc_to_src_jump.0,
        src_to_doc_jump.0.clone(),
    );

    std::thread::spawn(move || {
        world_actor.run();
    });

    let (data_plane_port_tx, data_plane_port_rx) = tokio::sync::oneshot::channel();
    let data_plane_addr = arguments.data_plane_host;
    let data_plane_handle = {
        let world_tx = world_mailbox.0.clone();
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
                let src_to_doc_rx = src_to_doc_jump.0.subscribe();
                let world_tx = world_tx.clone();
                let doc_watch_rx = doc_watch_rx.clone();
                let mut conn = accept_connection(stream).await;
                if enable_partial_rendering {
                    conn.send(Message::Binary("partial-rendering,true".into()))
                        .await
                        .unwrap();
                }
                let actor::webview::Channels { svg, render_full } =
                    actor::webview::WebviewActor::set_up_channels();
                let render_tx = render_full.0.clone();
                let webview_actor = actor::webview::WebviewActor::new(
                    conn,
                    svg.1,
                    src_to_doc_rx,
                    world_tx,
                    render_full.0,
                );
                tokio::spawn(async move {
                    webview_actor.run().await;
                });
                let render_actor =
                    actor::render::RenderActor::new(render_full.1, doc_watch_rx, svg.0);
                std::thread::spawn(move || {
                    render_actor.run();
                });
                let mut renderer_rx = renderer_mailbox.0.subscribe();
                tokio::spawn(async move {
                    while let Ok(msg) = renderer_rx.recv().await {
                        let Ok(_) = render_tx.send(msg) else {
                            break;
                        };
                    }
                });
            }
        })
    };

    let control_plane_addr = arguments.control_plane_host;
    let control_plane_handle = {
        let world_tx = world_mailbox.0.clone();
        let editor_rx = doc_to_src_jump.1;
        tokio::spawn(async move {
            let try_socket = TcpListener::bind(&control_plane_addr).await;
            let listener = try_socket.expect("Failed to bind");
            info!(
                "Control plane server listening on: {}",
                listener.local_addr().unwrap()
            );
            let (stream, _) = listener.accept().await.unwrap();
            let conn = accept_connection(stream).await;
            let editor_actor = EditorActor::new(editor_rx, conn, world_tx);
            editor_actor.run().await;
        })
    };
    let static_file_addr = arguments.static_file_host;
    if arguments.server_static_file || arguments.open_in_browser {
        let data_plane_port = data_plane_port_rx.await.unwrap();
        let make_service = make_service_fn(|_| {
            let data_plane_port = data_plane_port;
            async move {
                Ok::<_, hyper::http::Error>(service_fn(move |req| {
                    async move {
                        if req.uri().path() == "/" {
                            let html = HTML.replace(
                                "ws://127.0.0.1:23625",
                                format!("ws://127.0.0.1:{data_plane_port}").as_str(),
                            );
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
        if arguments.open_in_browser {
            if let Err(e) = open::that_detached(format!("http://{}", server.local_addr())) {
                error!("failed to open browser: {}", e);
            };
        }
        info!("Static file server listening on: {}", server.local_addr());
        if let Err(e) = server.await {
            error!("Static file server error: {}", e);
        }
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

/// Find the output location in the document for a cursor position.
pub fn jump_from_cursor(frames: &[Frame], source: &Source, cursor: usize) -> Option<Position> {
    let node = LinkedNode::new(source.root()).leaf_at(cursor)?;
    if node.kind() != SyntaxKind::Text {
        return None;
    }

    info!("jump_from_cursor: {:?} {:?}", node, node.span());

    let mut min_dis = u64::MAX;
    let mut p = Point::default();
    let mut ppage = 0usize;

    let span = node.span();
    for (i, frame) in frames.iter().enumerate() {
        let t_dis = min_dis;
        if let Some(pos) = find_in_frame(frame, span, &mut min_dis, &mut p) {
            return Some(Position {
                page: NonZeroUsize::new(i + 1).unwrap(),
                point: pos,
            });
        }
        if t_dis != min_dis {
            ppage = i;
        }
        info!("min_dis: {} {:?} {:?}", min_dis, ppage, p);
    }

    if min_dis == u64::MAX {
        return None;
    }

    Some(Position {
        page: NonZeroUsize::new(ppage + 1).unwrap(),
        point: p,
    })
}

/// Find the position of a span in a frame.
fn find_in_frame(frame: &Frame, span: Span, min_dis: &mut u64, p: &mut Point) -> Option<Point> {
    for (mut pos, item) in frame.items() {
        if let FrameItem::Group(group) = item {
            // TODO: Handle transformation.
            if let Some(point) = find_in_frame(&group.frame, span, min_dis, p) {
                return Some(point + pos);
            }
        }

        if let FrameItem::Text(text) = item {
            for glyph in &text.glyphs {
                if glyph.span.0 == span {
                    return Some(pos);
                }
                if glyph.span.0.id() == span.id() {
                    let dis = glyph.span.0.number().abs_diff(span.number());
                    if dis < *min_dis {
                        *min_dis = dis;
                        *p = pos;
                    }
                }
                pos.x += glyph.x_advance.at(text.size);
            }
        }
    }

    None
}
