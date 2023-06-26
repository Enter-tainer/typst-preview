mod args;
mod publisher;

use chrono::Datelike;
use clap::Parser;
use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::term::{self, termcolor};
use comemo::Prehashed;
use elsa::FrozenVec;

use futures::{SinkExt, StreamExt};
use log::{error, info};
use memmap2::Mmap;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use once_cell::sync::OnceCell;
use publisher::Publisher;
use same_file::Handle;

use serde::Serialize;
use siphasher::sip128::{Hasher128, SipHasher};
use std::cell::{Cell, RefCell, RefMut};
use std::collections::HashMap;
use std::fs::{self, File};
use std::hash::Hash;
use std::io::{self, Write};

use std::ops::DerefMut;
use std::path::{Path, PathBuf};

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
use typst::diag::{FileError, FileResult, SourceError, StrResult};
use typst::eval::{Datetime, Library};
use typst::font::{Font, FontBook, FontInfo, FontVariant};

use typst::syntax::{Source, SourceId};
use typst::util::{Buffer, PathExt};
use typst::World;

use walkdir::WalkDir;

type CodespanResult<T> = Result<T, CodespanError>;
type CodespanError = codespan_reporting::files::Error;

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

struct FontsSettings {
    /// The font paths
    font_paths: Vec<PathBuf>,

    /// Whether to include font variants
    variants: bool,
}

impl FontsSettings {
    /// Create font settings from the field values.
    pub fn new(font_paths: Vec<PathBuf>, variants: bool) -> Self {
        Self {
            font_paths,
            variants,
        }
    }

    /// Create a new font settings from the CLI arguments.
    ///
    /// # Panics
    /// Panics if the command is not a fonts command.
    pub fn with_arguments(args: CliArguments) -> Self {
        match args.command {
            Command::Fonts(command) => Self::new(args.font_paths, command.variants),
            _ => unreachable!(),
        }
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
        .filter_level(log::LevelFilter::Info)
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
    // Create the world that serves sources, fonts and files.
    let world = Arc::new(Mutex::new(SystemWorld::new(root, &arguments.font_paths)));
    {
        let arguments = arguments.clone();
        let publisher = publisher.clone();
        let world = world.clone();
        tokio::spawn(async move {
            let res = match &arguments.command {
                Command::Watch(_) => watch(world, command, publisher).await,
                Command::Fonts(_) => fonts(FontsSettings::with_arguments(arguments)),
            };

            if let Err(msg) = res {
                print_error(&msg).expect("failed to print error");
            }
        });
    }
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
                                            return;
                                        }
                                        let span = typst::syntax::Span::new(src_id, span_number);
                                        let world = world.lock().await;
                                        let source = world.source(src_id);
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
async fn watch(
    world: Arc<Mutex<SystemWorld>>,
    command: CompileSettings,
    publisher: Publisher<Document>,
) -> StrResult<()> {
    if let Some(doc) = compile_once(world.lock().await.deref_mut(), &command)? {
        publisher.publish(Arc::new(doc)).await;
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
        .watch(&world.lock().await.root, RecursiveMode::Recursive)
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
            if let Some(doc) = compile_once(world.lock().await.deref_mut(), &command)? {
                publisher.publish(Arc::new(doc)).await;
                comemo::evict(30);
            }
        }
    }
}

/// Compile a single time.
fn compile_once(world: &mut SystemWorld, command: &CompileSettings) -> StrResult<Option<Document>> {
    status(command, Status::Compiling).unwrap();

    world.reset();
    world.main = world
        .resolve(&command.input)
        .map_err(|err| err.to_string())?;

    match typst::compile(world) {
        // Export the images.
        Ok(document) => Ok(Some(document)),

        // Print diagnostics.
        Err(errors) => {
            status(command, Status::Error).unwrap();
            print_diagnostics(world, *errors).map_err(|_| "failed to print diagnostics")?;
            Ok(None)
        }
    }
}

/// Clear the terminal and render the status message.
fn status(command: &CompileSettings, status: Status) -> io::Result<()> {
    if !command.watch {
        return Ok(());
    }

    let _esc = 27 as char;
    let input = command.input.display();
    let time = chrono::offset::Local::now();
    let _timestamp = time.format("%H:%M:%S");
    let message = status.message();
    let _color = status.color();

    info!("{}: {}", input, message);
    Ok(())
}

/// The status in which the watcher can be.
#[allow(dead_code)]
enum Status {
    Compiling,
    Success,
    Error,
}

impl Status {
    fn message(&self) -> &str {
        match self {
            Self::Compiling => "compiling ...",
            Self::Success => "compiled successfully",
            Self::Error => "compiled with errors",
        }
    }

    fn color(&self) -> termcolor::ColorSpec {
        let styles = term::Styles::default();
        match self {
            Self::Error => styles.header_error,
            _ => styles.header_note,
        }
    }
}

/// Print diagnostic messages to the terminal.
fn print_diagnostics(
    world: &SystemWorld,
    errors: Vec<SourceError>,
) -> Result<(), codespan_reporting::files::Error> {
    let mut w = StandardStream::stderr(ColorChoice::Auto);
    let config = term::Config {
        tab_width: 2,
        ..Default::default()
    };

    for error in errors {
        // The main diagnostic.
        let range = error.range(world);
        let diag = Diagnostic::error()
            .with_message(error.message)
            .with_labels(vec![Label::primary(error.span.source(), range)]);

        term::emit(&mut w, &config, world, &diag)?;

        // Stacktrace-like helper diagnostics.
        for point in error.trace {
            let message = point.v.to_string();
            let help = Diagnostic::help()
                .with_message(message)
                .with_labels(vec![Label::primary(
                    point.span.source(),
                    world.source(point.span.source()).range(point.span),
                )]);

            term::emit(&mut w, &config, world, &help)?;
        }
    }

    Ok(())
}

/// Execute a font listing command.
fn fonts(command: FontsSettings) -> StrResult<()> {
    let mut searcher = FontSearcher::new();
    searcher.search_system();
    for path in &command.font_paths {
        searcher.search_dir(path)
    }
    for (name, infos) in searcher.book.families() {
        println!("{name}");
        if command.variants {
            for info in infos {
                let FontVariant {
                    style,
                    weight,
                    stretch,
                } = info.variant;
                println!("- Style: {style:?}, Weight: {weight:?}, Stretch: {stretch:?}");
            }
        }
    }

    Ok(())
}

/// A world that provides access to the operating system.
struct SystemWorld {
    root: PathBuf,
    library: Prehashed<Library>,
    book: Prehashed<FontBook>,
    fonts: Vec<FontSlot>,
    hashes: RefCell<HashMap<PathBuf, FileResult<PathHash>>>,
    paths: RefCell<HashMap<PathHash, PathSlot>>,
    sources: FrozenVec<Box<Source>>,
    today: Cell<Option<Datetime>>,
    main: SourceId,
}

/// Holds details about the location of a font and lazily the font itself.
struct FontSlot {
    path: PathBuf,
    index: u32,
    font: OnceCell<Option<Font>>,
}

/// Holds canonical data for all paths pointing to the same entity.
#[derive(Default)]
struct PathSlot {
    source: OnceCell<FileResult<SourceId>>,
    buffer: OnceCell<FileResult<Buffer>>,
}

impl SystemWorld {
    fn new(root: PathBuf, font_paths: &[PathBuf]) -> Self {
        let mut searcher = FontSearcher::new();
        searcher.search_system();

        #[cfg(feature = "embed-fonts")]
        searcher.add_embedded();

        for path in font_paths {
            searcher.search_dir(path)
        }

        Self {
            root,
            library: Prehashed::new(typst_library::build()),
            book: Prehashed::new(searcher.book),
            fonts: searcher.fonts,
            hashes: RefCell::default(),
            paths: RefCell::default(),
            sources: FrozenVec::new(),
            today: Cell::new(None),
            main: SourceId::detached(),
        }
    }
}

impl World for SystemWorld {
    fn root(&self) -> &Path {
        &self.root
    }

    fn library(&self) -> &Prehashed<Library> {
        &self.library
    }

    fn main(&self) -> &Source {
        self.source(self.main)
    }

    fn resolve(&self, path: &Path) -> FileResult<SourceId> {
        self.slot(path)?
            .source
            .get_or_init(|| {
                let buf = read(path)?;
                let text = String::from_utf8(buf)?;
                Ok(self.insert(path, text))
            })
            .clone()
    }

    fn source(&self, id: SourceId) -> &Source {
        &self.sources[id.as_u16() as usize]
    }

    fn book(&self) -> &Prehashed<FontBook> {
        &self.book
    }

    fn font(&self, id: usize) -> Option<Font> {
        let slot = &self.fonts[id];
        slot.font
            .get_or_init(|| {
                let data = self.file(&slot.path).ok()?;
                Font::new(data, slot.index)
            })
            .clone()
    }

    fn file(&self, path: &Path) -> FileResult<Buffer> {
        self.slot(path)?
            .buffer
            .get_or_init(|| read(path).map(Buffer::from))
            .clone()
    }

    fn today(&self, offset: Option<i64>) -> Option<Datetime> {
        if self.today.get().is_none() {
            let datetime = match offset {
                None => chrono::Local::now().naive_local(),
                Some(o) => (chrono::Utc::now() + chrono::Duration::hours(o)).naive_utc(),
            };

            self.today.set(Some(Datetime::from_ymd(
                datetime.year(),
                datetime.month().try_into().ok()?,
                datetime.day().try_into().ok()?,
            )?))
        }

        self.today.get()
    }
}

impl SystemWorld {
    fn slot(&self, path: &Path) -> FileResult<RefMut<PathSlot>> {
        let mut hashes = self.hashes.borrow_mut();
        let hash = match hashes.get(path).cloned() {
            Some(hash) => hash,
            None => {
                let hash = PathHash::new(path);
                if let Ok(canon) = path.canonicalize() {
                    hashes.insert(canon.normalize(), hash.clone());
                }
                hashes.insert(path.into(), hash.clone());
                hash
            }
        }?;

        Ok(std::cell::RefMut::map(self.paths.borrow_mut(), |paths| {
            paths.entry(hash).or_default()
        }))
    }

    fn insert(&self, path: &Path, text: String) -> SourceId {
        let id = SourceId::from_u16(self.sources.len() as u16);
        let source = Source::new(id, path, text);
        self.sources.push(Box::new(source));
        id
    }

    fn relevant(&self, event: &notify::Event) -> bool {
        match &event.kind {
            notify::EventKind::Any => {}
            notify::EventKind::Access(_) => return false,
            notify::EventKind::Create(_) => return true,
            notify::EventKind::Modify(kind) => match kind {
                notify::event::ModifyKind::Any => {}
                notify::event::ModifyKind::Data(_) => {}
                notify::event::ModifyKind::Metadata(_) => return false,
                notify::event::ModifyKind::Name(_) => return true,
                notify::event::ModifyKind::Other => return false,
            },
            notify::EventKind::Remove(_) => {}
            notify::EventKind::Other => return false,
        }

        event.paths.iter().any(|path| self.dependant(path))
    }

    fn dependant(&self, path: &Path) -> bool {
        self.hashes.borrow().contains_key(&path.normalize())
            || PathHash::new(path).map_or(false, |hash| self.paths.borrow().contains_key(&hash))
    }

    fn reset(&mut self) {
        self.sources.as_mut().clear();
        self.hashes.borrow_mut().clear();
        self.paths.borrow_mut().clear();
    }
}

/// A hash that is the same for all paths pointing to the same entity.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
struct PathHash(u128);

impl PathHash {
    fn new(path: &Path) -> FileResult<Self> {
        let f = |e| FileError::from_io(e, path);
        let handle = Handle::from_path(path).map_err(f)?;
        let mut state = SipHasher::new();
        handle.hash(&mut state);
        Ok(Self(state.finish128().as_u128()))
    }
}

/// Read a file.
fn read(path: &Path) -> FileResult<Vec<u8>> {
    let f = |e| FileError::from_io(e, path);
    if fs::metadata(path).map_err(f)?.is_dir() {
        Err(FileError::IsDirectory)
    } else {
        fs::read(path).map_err(f)
    }
}

impl<'a> codespan_reporting::files::Files<'a> for SystemWorld {
    type FileId = SourceId;
    type Name = std::path::Display<'a>;
    type Source = &'a str;

    fn name(&'a self, id: SourceId) -> CodespanResult<Self::Name> {
        Ok(World::source(self, id).path().display())
    }

    fn source(&'a self, id: SourceId) -> CodespanResult<Self::Source> {
        Ok(World::source(self, id).text())
    }

    fn line_index(&'a self, id: SourceId, given: usize) -> CodespanResult<usize> {
        let source = World::source(self, id);
        source
            .byte_to_line(given)
            .ok_or_else(|| CodespanError::IndexTooLarge {
                given,
                max: source.len_bytes(),
            })
    }

    fn line_range(&'a self, id: SourceId, given: usize) -> CodespanResult<std::ops::Range<usize>> {
        let source = World::source(self, id);
        source
            .line_to_range(given)
            .ok_or_else(|| CodespanError::LineTooLarge {
                given,
                max: source.len_lines(),
            })
    }

    fn column_number(&'a self, id: SourceId, _: usize, given: usize) -> CodespanResult<usize> {
        let source = World::source(self, id);
        source.byte_to_column(given).ok_or_else(|| {
            let max = source.len_bytes();
            if given <= max {
                CodespanError::InvalidCharBoundary { given }
            } else {
                CodespanError::IndexTooLarge { given, max }
            }
        })
    }
}

/// Searches for fonts.
struct FontSearcher {
    book: FontBook,
    fonts: Vec<FontSlot>,
}

impl FontSearcher {
    /// Create a new, empty system searcher.
    fn new() -> Self {
        Self {
            book: FontBook::new(),
            fonts: vec![],
        }
    }

    /// Add fonts that are embedded in the binary.
    #[cfg(feature = "embed-fonts")]
    fn add_embedded(&mut self) {
        let mut add = |bytes: &'static [u8]| {
            let buffer = Buffer::from_static(bytes);
            for (i, font) in Font::iter(buffer).enumerate() {
                self.book.push(font.info().clone());
                self.fonts.push(FontSlot {
                    path: PathBuf::new(),
                    index: i as u32,
                    font: OnceCell::from(Some(font)),
                });
            }
        };

        // Embed default fonts.
        add(include_bytes!("../assets/fonts/LinLibertine_R.ttf"));
        add(include_bytes!("../assets/fonts/LinLibertine_RB.ttf"));
        add(include_bytes!("../assets/fonts/LinLibertine_RBI.ttf"));
        add(include_bytes!("../assets/fonts/LinLibertine_RI.ttf"));
        add(include_bytes!("../assets/fonts/NewCMMath-Book.otf"));
        add(include_bytes!("../assets/fonts/NewCMMath-Regular.otf"));
        add(include_bytes!("../assets/fonts/NewCM10-Regular.otf"));
        add(include_bytes!("../assets/fonts/NewCM10-Bold.otf"));
        add(include_bytes!("../assets/fonts/NewCM10-Italic.otf"));
        add(include_bytes!("../assets/fonts/NewCM10-BoldItalic.otf"));
        add(include_bytes!("../assets/fonts/DejaVuSansMono.ttf"));
        add(include_bytes!("../assets/fonts/DejaVuSansMono-Bold.ttf"));
        add(include_bytes!("../assets/fonts/DejaVuSansMono-Oblique.ttf"));
        add(include_bytes!(
            "../assets/fonts/DejaVuSansMono-BoldOblique.ttf"
        ));
    }

    /// Search for fonts in the linux system font directories.
    #[cfg(all(unix, not(target_os = "macos")))]
    fn search_system(&mut self) {
        self.search_dir("/usr/share/fonts");
        self.search_dir("/usr/local/share/fonts");

        if let Some(dir) = dirs::font_dir() {
            self.search_dir(dir);
        }
    }

    /// Search for fonts in the macOS system font directories.
    #[cfg(target_os = "macos")]
    fn search_system(&mut self) {
        self.search_dir("/Library/Fonts");
        self.search_dir("/Network/Library/Fonts");
        self.search_dir("/System/Library/Fonts");

        if let Some(dir) = dirs::font_dir() {
            self.search_dir(dir);
        }
    }

    /// Search for fonts in the Windows system font directories.
    #[cfg(windows)]
    fn search_system(&mut self) {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());

        self.search_dir(Path::new(&windir).join("Fonts"));

        if let Some(roaming) = dirs::config_dir() {
            self.search_dir(roaming.join("Microsoft\\Windows\\Fonts"));
        }

        if let Some(local) = dirs::cache_dir() {
            self.search_dir(local.join("Microsoft\\Windows\\Fonts"));
        }
    }

    /// Search for all fonts in a directory recursively.
    fn search_dir(&mut self, path: impl AsRef<Path>) {
        for entry in WalkDir::new(path)
            .follow_links(true)
            .sort_by(|a, b| a.file_name().cmp(b.file_name()))
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("ttf" | "otf" | "TTF" | "OTF" | "ttc" | "otc" | "TTC" | "OTC"),
            ) {
                self.search_file(path);
            }
        }
    }

    /// Index the fonts in the file at the given path.
    fn search_file(&mut self, path: impl AsRef<Path>) {
        let path = path.as_ref();
        if let Ok(file) = File::open(path) {
            if let Ok(mmap) = unsafe { Mmap::map(&file) } {
                for (i, info) in FontInfo::iter(&mmap).enumerate() {
                    self.book.push(info);
                    self.fonts.push(FontSlot {
                        path: path.into(),
                        index: i as u32,
                        font: OnceCell::new(),
                    });
                }
            }
        }
    }
}
