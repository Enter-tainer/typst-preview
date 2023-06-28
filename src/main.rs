mod args;
mod publisher;

use clap::Parser;
use codespan_reporting::term::{self, termcolor};

use futures::{SinkExt, StreamExt};
use log::{error, info};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use publisher::Publisher;

use serde::Serialize;
use std::io::{self, Write};

use std::path::PathBuf;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use termcolor::{ColorChoice, StandardStream, WriteColor};
use tokio::net::{TcpListener, TcpStream};
use typst::doc::Document;
use typst_ts_svg_exporter::IncrementalSvgExporter;

use crate::args::{CliArguments, Command, CompileCommand};
use crate::publisher::PublisherImpl;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use typst::diag::StrResult;

use typst::syntax::SourceId;
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
        let _watch = matches!(args.command, Command::Watch(_));
        let CompileCommand { input } = match args.command {
            Command::Watch(command) => command,
            _ => unreachable!(),
        };
        Self::new(input, true, args.root, args.font_paths)
    }
}

struct Client {
    conn: Arc<Mutex<WebSocketStream<TcpStream>>>,
    publisher: Publisher<Document>,
    renderer: Arc<Mutex<IncrementalSvgExporter>>,
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

/// Entry point.
#[tokio::main]
async fn main() {
    let _ = env_logger::builder()
        .filter_module("typst_ws", log::LevelFilter::Info)
        .try_init();
    let arguments = CliArguments::parse();
    info!("Arguments: {:#?}", arguments);
    let publisher: Publisher<Document> = PublisherImpl::new().into();
    let command = CompileSettings::with_arguments(arguments.clone());
    let root = if let Some(root) = &command.root {
        root.clone()
    } else if let Some(dir) = command
        .input
        .canonicalize()
        .ok()
        .as_ref()
        .and_then(|path| path.parent())
    {
        dir.into()
    } else {
        PathBuf::new()
    };

    let compile_driver = {
        let world = TypstSystemWorld::new(CompileOpts {
            root_dir: root.clone(),
            font_paths: arguments.font_paths.clone(),
            with_embedded_fonts: EMBEDDED_FONT.to_owned(),
            ..CompileOpts::default()
        });

        CompileDriver::new(world).with_entry_file(command.input.clone())
    };

    // Create the world that serves sources, fonts and files.
    let world = Arc::new(Mutex::new(compile_driver));
    {
        let arguments = arguments.clone();
        let publisher = publisher.clone();
        let world = world.clone();
        tokio::spawn(async move {
            let res = match &arguments.command {
                Command::Watch(_) => watch(world, publisher).await,
                Command::Fonts(_) => todo!(), // fonts(FontsSettings::with_arguments(arguments)),
            };

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

    let data_plane_addr = arguments
        .data_plane_host
        .unwrap_or_else(|| "127.0.0.1:23625".to_string());
    let (jump_tx, mut jump_rx) = tokio::sync::mpsc::channel(8);
    let data_plane_handle = tokio::spawn(async move {
        // Create the event loop and TCP listener we'll accept connections on.
        let try_socket = TcpListener::bind(&data_plane_addr).await;
        let listener = try_socket.expect("Failed to bind");
        info!(
            "Data plane server listening on: {}",
            listener.local_addr().unwrap()
        );

        let active_client_count = Arc::new(AtomicUsize::new(0));
        while let Ok((stream, _)) = listener.accept().await {
            let conn = accept_connection(stream).await;
            {
                let mut r = IncrementalSvgExporter::default();
                r.set_should_attach_debug_info(true);
                let client = Client {
                    conn: Arc::new(Mutex::new(conn)),
                    publisher: publisher.clone(),
                    renderer: Arc::new(Mutex::new(r)),
                };
                let active_client_count = active_client_count.clone();
                let publisher = publisher.clone();
                let world = world.clone();
                let jump_tx = jump_tx.clone();
                tokio::spawn(async move {
                    active_client_count.fetch_add(1, Ordering::SeqCst);
                    loop {
                        tokio::select! {
                            msg = Client::poll_ws_events(client.conn.clone()) => {
                                if let Some(msg) = msg {
                                    if msg == "current" {
                                        let mut renderer = client.renderer.lock().await;
                                        if let Some(res) = renderer.render_current() {
                                            client.conn.lock().await.send(Message::Text(res)).await.unwrap();
                                        } else {
                                            let latest_doc = publisher.latest().await;
                                            if latest_doc.is_some() {
                                                let render_result = renderer.render(latest_doc.unwrap());
                                                client.conn.lock().await.send(Message::Text(render_result)).await.unwrap();
                                            } else {
                                                client.conn.lock().await.send(Message::Text("current not avalible".into())).await.unwrap();
                                            }
                                        }
                                    } else if msg.starts_with("srclocation") {
                                        let location = msg.split(' ').nth(1).unwrap();
                                        let id = u64::from_str_radix(location, 16).unwrap();
                                        // get the high 16bits and the low 48bits
                                        let (src_id, span_number) = (id >> 48, id & 0x0000FFFFFFFFFFFF);
                                        let src_id = SourceId::from_u16(src_id as u16);
                                        if src_id == SourceId::detached() || span_number <= 1 {
                                            continue;
                                        }
                                        let span = typst::syntax::Span::new(src_id, span_number);
                                        let world = world.lock().await;
                                        let source = world.world.source(src_id);
                                        let range = source.range(span);
                                        let jump = JumpInfo::from_option (
                                            source.path().to_string_lossy().to_string(),
                                            (source.byte_to_line(range.start), source.byte_to_column(range.start)),
                                            (source.byte_to_line(range.end), source.byte_to_column(range.end))
                                        );
                                        let _ = jump_tx.send(jump).await;
                                    } else {
                                        client.conn.lock().await.send(Message::Text(format!("unknown command {msg}"))).await.unwrap();
                                    }
                                } else {
                                    break;
                                }
                            },
                            doc = Client::poll_watch_events(client.publisher.clone()) => {
                                let svg = client.renderer.lock().await.render(doc);
                                client.conn.lock().await.send(Message::Text(svg)).await.unwrap();
                            },
                        }
                    }
                    info!("Peer closed WebSocket connection.");
                    let prev = active_client_count.fetch_sub(1, Ordering::SeqCst);
                    if prev == 1 {
                        // There is no clients left. Wait 30s and shutdown if no new clients connect.
                        tokio::time::sleep(Duration::from_secs(30)).await;
                        if active_client_count.load(Ordering::SeqCst) == 0 {
                            info!("No active clients. Shutting down.");
                            std::process::exit(0);
                        }
                    }
                });
            }
        }
    });

    let control_plane_addr = arguments
        .control_plane_host
        .unwrap_or_else(|| "127.0.0.1:23626".to_string());
    let control_plane_handle = tokio::spawn(async move {
        let try_socket = TcpListener::bind(&control_plane_addr).await;
        let listener = try_socket.expect("Failed to bind");
        info!(
            "Control plane server listening on: {}",
            listener.local_addr().unwrap()
        );
        let (stream, _) = listener.accept().await.unwrap();
        let mut conn = accept_connection(stream).await;
        while let Some(jump) = jump_rx.recv().await {
            info!("sending jump info to editor: {:?}", &jump);
            let res = conn
                .send(Message::Text(serde_json::to_string(&jump).unwrap()))
                .await;
            if res.is_err() {
                break;
            }
        }
    });
    let static_file_addr = arguments
        .static_file_host
        .unwrap_or_else(|| "127.0.0.1:23267".to_string());
    if let Some(path) = arguments.static_file_path {
        let server = actix_web::HttpServer::new(move || {
            actix_web::App::new()
                .service(actix_files::Files::new("/", &path).index_file("index.html"))
            // Enable the logger.
            // .wrap(actix_web::middleware::Logger::default())
        });
        open::that_detached(format!("http://{}", static_file_addr)).unwrap();
        server
            .bind(&static_file_addr)
            .unwrap()
            .workers(1)
            .run()
            .await
            .unwrap();
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
        .watch(world.lock().await.world.root(), RecursiveMode::Recursive)
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
