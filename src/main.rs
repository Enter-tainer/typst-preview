mod actor;
mod args;
use clap::Parser;
use codespan_reporting::term::{self, termcolor};

use futures::{SinkExt, StreamExt};
use hyper::http::Error;
use hyper::service::{make_service_fn, service_fn};
use log::{error, info, warn};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, Write};
use typst::geom::Point;

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use termcolor::{ColorChoice, StandardStream, WriteColor};
use tokio::net::{TcpListener, TcpStream};
use typst::doc::{Document, Frame, FrameItem, Position};
use typst_ts_svg_exporter::IncrSvgDocServer;

use crate::args::CliArguments;
use crate::publisher::PublisherImpl;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use typst::diag::StrResult;

use typst::syntax::{FileId, LinkedNode, Source, Span, SyntaxKind};
use typst::World;
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

#[derive(Debug, Deserialize)]
#[serde(tag = "event")]
enum ControlPlaneMessage {
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
}

const HTML: &str = include_str!("../addons/vscode/out/frontend/index.html");

/// Entry point.
#[tokio::main]
async fn main() {
    let _ = env_logger::builder()
        .filter_module("typst_preview", log::LevelFilter::Info)
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
    let world = Arc::new(Mutex::new(compile_driver));
    {
        let world = world.clone();
        tokio::spawn(async move {});
    }

    let (data_plane_port_tx, data_plane_port_rx) = tokio::sync::oneshot::channel();
    let data_plane_addr = arguments.data_plane_host;
    let data_plane_handle = {
        let world = world.clone();
        tokio::spawn(async move {
            // Create the event loop and TCP listener we'll accept connections on.
            let try_socket = TcpListener::bind(&data_plane_addr).await;
            let listener = try_socket.expect("Failed to bind");
            info!(
                "Data plane server listening on: {}",
                listener.local_addr().unwrap()
            );
            let _ = data_plane_port_tx.send(listener.local_addr().unwrap().port());
            let active_client_count = Arc::new(AtomicUsize::new(0));
        })
    };

    let control_plane_addr = arguments.control_plane_host;
    let control_plane_handle = tokio::spawn(async move {
        let try_socket = TcpListener::bind(&control_plane_addr).await;
        let listener = try_socket.expect("Failed to bind");
        info!(
            "Control plane server listening on: {}",
            listener.local_addr().unwrap()
        );
        let (stream, _) = listener.accept().await.unwrap();
        let conn = accept_connection(stream).await;
    });
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

/// Print an application-level error (independent from a source file).
fn print_error(msg: &str) -> io::Result<()> {
    let mut w = StandardStream::stderr(ColorChoice::Auto);
    let styles = term::Styles::default();

    w.set_color(&styles.header_error)?;
    write!(w, "error")?;

    w.reset()?;
    writeln!(w, ": {msg}.")
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
