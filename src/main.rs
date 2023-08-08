mod args;
mod publisher;

use clap::Parser;
use codespan_reporting::term::{self, termcolor};

use futures::{SinkExt, StreamExt};
use hyper::http::Error;
use hyper::service::{make_service_fn, service_fn};
use log::{error, info, warn};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use publisher::Publisher;

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
use typst_library::prelude::FileId;

use typst::syntax::{LinkedNode, Source, Span, SyntaxKind};
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

struct Client {
    conn: Arc<Mutex<WebSocketStream<TcpStream>>>,
    publisher: Publisher<Document>,
    doc_server: Arc<Mutex<IncrSvgDocServer>>,
}

impl Client {
    pub async fn poll_ws_events(conn: Arc<Mutex<WebSocketStream<TcpStream>>>) -> Option<String> {
        if let Some(Ok(Message::Text(text))) = conn.lock().await.next().await {
            Some(text)
        } else {
            None
        }
    }

    pub async fn poll_watch_events(publisher: Publisher<Document>) -> Arc<Document> {
        publisher.wait().await
    }
}

#[derive(Debug, Serialize)]
struct JumpInfo {
    filepath: String,
    start: Option<(usize, usize)>, // row, column
    end: Option<(usize, usize)>,
}

impl JumpInfo {
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
struct SrcToDocJumpRequest {
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
struct MemoryFiles {
    files: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct MemoryFilesShort {
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
    EditorScrollTo(JumpInfo),
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
    let doc_publisher: Publisher<Document> = PublisherImpl::new().into();
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

    // Create the world that serves sources, fonts and files.
    let world = Arc::new(Mutex::new(compile_driver));
    {
        let publisher = doc_publisher.clone();
        let world = world.clone();
        tokio::spawn(async move {
            let res = watch(world, publisher).await;

            if let Err(msg) = res {
                print_error(&msg).expect("failed to print error");
            }
        });
    }

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        info!("Ctrl-C received, exiting");
        std::process::exit(0);
    });

    let (data_plane_port_tx, data_plane_port_rx) = tokio::sync::oneshot::channel();
    let data_plane_addr = arguments.data_plane_host;
    let (doc_to_src_jump_tx, mut doc_to_src_jump_rx) = tokio::sync::mpsc::channel(8);
    let src_to_doc_jump_publisher: Publisher<typst::doc::Position> = PublisherImpl::new().into();
    let data_plane_handle = {
        let world = world.clone();
        let doc_publisher = doc_publisher.clone();
        let src_to_doc_jump_publisher = src_to_doc_jump_publisher.clone();
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
            loop {
                let listen_timeout =
                    tokio::time::timeout(Duration::from_secs(10), listener.accept()).await;
                if listen_timeout.is_err() {
                    if active_client_count.load(Ordering::SeqCst) == 0 {
                        info!("No active clients. Shutting down.");
                        std::process::exit(0);
                    }
                    continue;
                } else if let Ok(Ok((stream, _))) = listen_timeout {
                    let conn = accept_connection(stream).await;
                    {
                        let mut r = IncrSvgDocServer::default();
                        r.set_should_attach_debug_info(true);
                        let client = Client {
                            conn: Arc::new(Mutex::new(conn)),
                            publisher: doc_publisher.clone(),
                            doc_server: Arc::new(Mutex::new(r)),
                        };
                        let active_client_count = active_client_count.clone();
                        let doc_publisher = doc_publisher.clone();
                        let world = world.clone();
                        let jump_tx = doc_to_src_jump_tx.clone();
                        let src_to_doc_jump_publisher = src_to_doc_jump_publisher.clone();
                        tokio::spawn(async move {
                            active_client_count.fetch_add(1, Ordering::SeqCst);
                            if enable_partial_rendering {
                                client
                                    .conn
                                    .lock()
                                    .await
                                    .send(Message::Binary("partial-rendering,true".into()))
                                    .await
                                    .unwrap();
                            }
                            loop {
                                tokio::select! {
                                    msg = Client::poll_ws_events(client.conn.clone()) => {
                                        if let Some(msg) = msg {
                                            if msg == "current" {
                                                let mut renderer = client.doc_server.lock().await;
                                                if let Some(res) = renderer.pack_current() {
                                                    client.conn.lock().await.send(Message::Binary(res)).await.unwrap();
                                                } else {
                                                    let latest_doc = doc_publisher.latest().await;
                                                    if latest_doc.is_some() {
                                                        let render_result = renderer.pack_delta(latest_doc.unwrap());
                                                        client.conn.lock().await.send(Message::Binary(render_result)).await.unwrap();
                                                    } else {
                                                        client.conn.lock().await.send(Message::Text("current not avalible".into())).await.unwrap();
                                                    }
                                                }
                                            } else if msg.starts_with("srclocation") {
                                                let location = msg.split(' ').nth(1).unwrap();
                                                let id = u64::from_str_radix(location, 16).unwrap();
                                                // get the high 16bits and the low 48bits
                                                let (src_id, span_number) = (id >> 48, id & 0x0000FFFFFFFFFFFF);
                                                let src_id = FileId::from_u16(src_id as u16);
                                                if src_id == FileId::detached() || span_number <= 1 {
                                                    continue;
                                                }
                                                let span = typst::syntax::Span::new(src_id, span_number);
                                                let world = world.lock().await;
                                                let source = world.world.source(src_id).unwrap();
                                                let range = source.find(span).unwrap().range();
                                                let file_path = world.world.path_for_id(src_id).unwrap();
                                                let jump = JumpInfo::from_option (
                                                    file_path.to_string_lossy().to_string(),
                                                    (source.byte_to_line(range.start), source.byte_to_column(range.start)),
                                                    (source.byte_to_line(range.end), source.byte_to_column(range.end))
                                                );
                                                let _ = jump_tx.send(jump).await;
                                            } else {
                                                client.conn.lock().await.send(Message::Binary(format!("unknown command {msg}").into())).await.unwrap();
                                            }
                                        } else {
                                            break;
                                        }
                                    },
                                    doc = Client::poll_watch_events(client.publisher.clone()) => {
                                        let svg = client.doc_server.lock().await.pack_delta(doc);
                                        client.conn.lock().await.send(Message::Binary(svg)).await.unwrap();
                                    },
                                    // todo: bug, all of the clients will receive the same jump info
                                    pos = src_to_doc_jump_publisher.wait() => {
                                        let mut conn = client.conn.lock().await;
                                        let jump_info = format!("jump,{} {} {}", pos.page, pos.point.x.to_pt(), pos.point.y.to_pt());
                                        info!("sending src2doc jump info {}", jump_info);
                                        conn.send(Message::Binary(jump_info.into_bytes())).await.unwrap();
                                    }
                                }
                            }
                            info!("Peer closed WebSocket connection.");
                            let prev = active_client_count.fetch_sub(1, Ordering::SeqCst);
                            if prev == 1 {
                                // There is no clients left. Wait 10s and shutdown if no new clients connect.
                                tokio::time::sleep(Duration::from_secs(10)).await;
                                if active_client_count.load(Ordering::SeqCst) == 0 {
                                    info!("No active clients. Shutting down.");
                                    std::process::exit(0);
                                }
                            }
                        });
                    }
                }
            }
        })
    };

    let control_plane_addr = arguments.control_plane_host;
    let control_plane_handle = tokio::spawn(async move {
        let doc_publisher = doc_publisher.clone();
        let try_socket = TcpListener::bind(&control_plane_addr).await;
        let listener = try_socket.expect("Failed to bind");
        info!(
            "Control plane server listening on: {}",
            listener.local_addr().unwrap()
        );
        let (stream, _) = listener.accept().await.unwrap();
        let mut conn = accept_connection(stream).await;

        // todo: when the compiler crashed, sync again
        conn.send(Message::Text(
            serde_json::to_string(&ControlPlaneResponse::SyncEditorChanges(())).unwrap(),
        ))
        .await
        .unwrap();

        loop {
            tokio::select! {
                Some(jump) = doc_to_src_jump_rx.recv() => {
                    let jump = ControlPlaneResponse::EditorScrollTo(jump);
                    let res = conn
                        .send(Message::Text(serde_json::to_string(&jump).unwrap()))
                        .await;
                    info!("sent doc2src jump info to editor: {:?}", &jump);
                    if res.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Text(msg))) = conn.next() => {
                    let Ok(msg) = serde_json::from_str::<ControlPlaneMessage>(&msg) else {
                        warn!("failed to parse jump request: {:?}", msg);
                        continue;
                    };

                    match msg {
                        ControlPlaneMessage::SrcToDocJump(msg) => {
                            let world = world.lock().await;
                            let Ok(relative_path) = Path::new(&msg.filepath).strip_prefix(&world.world.root) else {
                                warn!("failed to strip prefix: {:?}, {:?}. currently jump from typst packages is not supported", msg, world.world.root);
                                continue;
                            };
                            // fixme: in typst v0.6.0, all relative path should start with `/`
                            let source_id = FileId::new(None, &Path::new("/").join(relative_path));
                            let Ok(source) = world.world.source(source_id) else {
                                warn!("filed to resolve source from path: {:?}", msg);
                                continue;
                            };
                            let Some(cursor) = msg.to_byte_offset(&source) else {
                                warn!("failed to resolve cursor: {:?}", msg);
                                continue;
                            };
                            let Some(doc) = doc_publisher.latest().await else {
                                warn!("latest doc is empty");
                                continue;
                            };
                            if let Some(pos) = jump_from_cursor(&doc.pages, &source, cursor) {
                                src_to_doc_jump_publisher.publish(pos.into()).await;
                            } else {
                                warn!("failed to jump from cursor: {:?}, {:?}, {:?}", msg, source, cursor);
                            }
                        }
                        ControlPlaneMessage::SyncMemoryFiles(msg) => {
                            info!("sync memory files {:?}", msg.files.keys());
                            let mut world = world.lock().await;
                            world.world.reset_shadow();
                            world.world.reset();
                            for (fc, content) in msg.files.iter() {
                                let pb = Path::new(fc).to_owned();
                                let id = world.id_for_path(pb.clone());
                                world.world.resolve_with(pb, id, content).unwrap();
                            }
                            // todo: refactor
                            if let Some(doc) = world.with_compile_diag::<true, _>(CompileDriver::compile) {
                                doc_publisher.publish(Arc::new(doc)).await;
                                comemo::evict(30);
                            }
                        }
                        ControlPlaneMessage::UpdateMemoryFiles(msg) => {
                            info!("update memory files {:?}", msg.files.keys());
                            let mut world = world.lock().await;
                            // don't reset shadow
                            world.world.reset();
                            for (fc, content) in msg.files.iter() {
                                let pb = Path::new(fc).to_owned();
                                let id = world.id_for_path(pb.clone());
                                world.world.resolve_with(pb, id, content).unwrap();
                            }
                            if let Some(doc) = world.with_compile_diag::<true, _>(CompileDriver::compile) {
                                doc_publisher.publish(Arc::new(doc)).await;
                                comemo::evict(30);
                            }
                        }
                        ControlPlaneMessage::RemoveMemoryFiles(msg) => {
                            info!("close memory files {:?}", msg.files);
                            let mut world = world.lock().await;
                            // don't reset shadow
                            world.world.reset();
                            for fc in msg.files.iter() {
                                let pb = Path::new(fc);
                                world.world.remove_shadow(pb);
                            }
                            if let Some(doc) = world.with_compile_diag::<true, _>(CompileDriver::compile) {
                                doc_publisher.publish(Arc::new(doc)).await;
                                comemo::evict(30);
                            }
                        }
                    }

                }
            }
        }
    });
    let static_file_addr = arguments.open_in_browser_host;
    if arguments.open_in_browser {
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
        open::that_detached(format!("http://{}", server.local_addr())).unwrap();
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

/// Execute a compilation command.
async fn watch(world: Arc<Mutex<CompileDriver>>, publisher: Publisher<Document>) -> StrResult<()> {
    {
        let mut world = world.lock().await;
        if let Some(doc) = world.with_compile_diag::<true, _>(CompileDriver::compile) {
            publisher.publish(Arc::new(doc)).await;
        }
    }
    // Setup file watching.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<notify::Event, _>| match res {
            Ok(e) => {
                tx.send(e).unwrap();
            }
            Err(e) => error!("watch error: {:#}", e),
        },
        notify::Config::default(),
    )
    .map_err(|_| "failed to watch directory")?;
    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    watcher
        .watch(&world.lock().await.world.root, RecursiveMode::Recursive)
        .unwrap();

    // Handle events.
    info!("start watching files...");
    loop {
        let mut recompile = false;
        let mut events: Vec<Option<notify::Event>> = vec![];
        while let Ok(e) =
            tokio::time::timeout(tokio::time::Duration::from_millis(100), rx.recv()).await
        {
            events.push(e);
        }
        {
            let world = world.lock().await;
            for event in events.into_iter().flatten() {
                recompile |= world.relevant(&event);
            }
            drop(world);
        }
        if recompile {
            let mut world = world.lock().await;
            if let Some(doc) = world.with_compile_diag::<true, _>(CompileDriver::compile) {
                publisher.publish(Arc::new(doc)).await;
                comemo::evict(30);
            }
        }
    }
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
