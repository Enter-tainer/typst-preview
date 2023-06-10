mod args;

use chrono::Datelike;
use clap::Parser;
use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::term::{self, termcolor};
use comemo::Prehashed;
use elsa::FrozenVec;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use log::{error, info};
use memmap2::Mmap;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use once_cell::unsync::OnceCell;
use same_file::Handle;
use serde::{Deserialize, Serialize};
use siphasher::sip128::{Hasher128, SipHasher};
use std::cell::{RefCell, RefMut, Cell};
use std::collections::HashMap;
use std::fs::{self, File};
use std::hash::Hash;
use std::io::{self, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::str::FromStr;
use std::sync::Arc;
use termcolor::{ColorChoice, StandardStream, WriteColor};
use tokio::net::{TcpListener, TcpStream};
use typst::doc::Frame;

use tokio::sync::{Mutex, RwLock};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use typst::diag::{FileError, FileResult, SourceError, StrResult};
use typst::eval::{Library, Datetime};
use typst::font::{Font, FontBook, FontInfo, FontVariant};
use typst::geom::RgbaColor;
use typst::syntax::{Source, SourceId};
use typst::util::{Buffer, PathExt};
use typst::World;
use walkdir::WalkDir;

use crate::args::{CliArguments, Command, CompileCommand};

type CodespanResult<T> = Result<T, CodespanError>;
type CodespanError = codespan_reporting::files::Error;

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
#[derive(Debug)]
struct Client {
    tx: Mutex<SplitSink<WebSocketStream<TcpStream>, Message>>,
    rx: Mutex<SplitStream<WebSocketStream<TcpStream>>>,
    visible_range: Mutex<Option<Range<usize>>>,
    dirty_marks: Mutex<Vec<bool>>,
    pages_hashes: Mutex<Vec<u128>>,
}

impl Client {
    async fn update_hashes(&self, new_hashes: Vec<u128>) {
        let mut dirty_marks = self.dirty_marks.lock().await;
        dirty_marks.resize(new_hashes.len(), true);
        let mut pages_hashes = self.pages_hashes.lock().await;
        for (i, &hash) in new_hashes.iter().enumerate() {
            if i < pages_hashes.len() && hash != pages_hashes[i] {
                dirty_marks[i] = true;
            }
        }
        *pages_hashes = new_hashes;
        let updated_pages = dirty_marks
            .iter()
            .enumerate()
            .filter_map(|(i, &dirty)| if dirty { Some(i) } else { None })
            .collect::<Vec<_>>();
        info!("updated pages: {:?}", updated_pages);
    }
    async fn send(&self, imgs: &[tiny_skia::Pixmap]) -> bool {
        #[derive(Debug, Serialize)]
        struct Info {
            page_total: usize,
            page_numbers: Vec<usize>,
            width: u32,
            height: u32,
        }
        let visible_range = self
            .visible_range
            .lock()
            .await
            .clone()
            .unwrap_or(0..imgs.len());
        let mut pages_to_send = vec![];
        {
            let dirty_marks = self.dirty_marks.lock().await;
            for i in visible_range.clone() {
                if let Some(dirty_mark) = dirty_marks.get(i) {
                    if *dirty_mark {
                        pages_to_send.push(i);
                    }
                }
            }
        }
        if pages_to_send.is_empty() {
            return true;
        }
        let pages_to_send = pages_to_send;
        let json = serde_json::to_string(&Info {
            page_total: imgs.len(),
            page_numbers: pages_to_send.clone(),
            width: imgs[0].width(),
            height: imgs[0].height(),
        })
        .unwrap();
        if let Err(err) = self.tx.lock().await.send(Message::Text(json)).await {
            error!("failed to send to client: {}", err);
            return false;
        }
        let mut dirty_marks = self.dirty_marks.lock().await;
        for &i in pages_to_send.iter() {
            dirty_marks[i] = false;
            let _ = self
                .tx
                .lock()
                .await
                .send(Message::Binary(imgs[i].data().to_vec()))
                .await; // don't care result here
        }
        info!(
            "visible range {:?}, sent pages {:?}",
            visible_range, pages_to_send
        );
        true
    }

    async fn poll_visible_range(&self, imgs: Arc<RwLock<Vec<tiny_skia::Pixmap>>>) -> bool {
        while let Some(msg) = self.rx.lock().await.next().await {
            #[derive(Debug, Deserialize)]
            struct VisibleRangeInfo {
                page_start: usize,
                page_end: usize,
            }
            match msg {
                Ok(msg) => {
                    if let Message::Text(text) = msg {
                        let range: VisibleRangeInfo = serde_json::from_str(&text).unwrap();
                        *self.visible_range.lock().await = Some(range.page_start..range.page_end);
                        self.send(imgs.read().await.as_slice()).await;
                    }
                }
                Err(err) => {
                    error!("failed to receive from client: {}", err);
                    return false;
                }
            }
        }
        error!("client disconnected");
        exit(-1);
    }
}

/// Entry point.
#[tokio::main]
async fn main() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .try_init();
    let arguments = CliArguments::parse();
    let conns = Arc::new(Mutex::new(Vec::new()));
    let imgs = Arc::new(RwLock::new(Vec::new()));
    {
        let conns = conns.clone();
        let imgs = imgs.clone();
        let arguments = arguments.clone();
        tokio::spawn(async {
            let res = match &arguments.command {
                Command::Watch(_) => {
                    watch(CompileSettings::with_arguments(arguments), conns, imgs).await
                }
                Command::Fonts(_) => fonts(FontsSettings::with_arguments(arguments)),
            };

            if let Err(msg) = res {
                print_error(&msg).expect("failed to print error");
            }
        });
    }
    let addr = arguments
        .host
        .unwrap_or_else(|| "127.0.0.1:23625".to_string());

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    info!("Listening on: {}", listener.local_addr().unwrap());

    while let Ok((stream, _)) = listener.accept().await {
        let conn = accept_connection(stream).await;
        {
            let (tx, rx) = conn.split();
            let client = Arc::new(Client {
                tx: Mutex::new(tx),
                rx: Mutex::new(rx),
                visible_range: Mutex::new(None),
                dirty_marks: Mutex::new(Vec::new()),
                pages_hashes: Mutex::new(Vec::new()),
            });
            conns.lock().await.push(client.clone());
            {
                let imgs = imgs.clone();
                tokio::spawn(async move { client.poll_visible_range(imgs).await });
            }
        }
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

/// Print an application-level error (independent from a source file).
fn print_error(msg: &str) -> io::Result<()> {
    let mut w = StandardStream::stderr(ColorChoice::Auto);
    let styles = term::Styles::default();

    w.set_color(&styles.header_error)?;
    write!(w, "error")?;

    w.reset()?;
    writeln!(w, ": {msg}.")
}
fn with_index<T, F>(mut f: F) -> impl FnMut(&T) -> bool
where
    F: FnMut(usize, &T) -> bool,
{
    let mut i = 0;
    move |item| (f(i, item), i += 1).0
}

/// Execute a compilation command.
async fn watch(
    command: CompileSettings,
    conns: Arc<Mutex<Vec<Arc<Client>>>>,
    imgs: Arc<RwLock<Vec<tiny_skia::Pixmap>>>,
) -> StrResult<()> {
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
    let mut world = SystemWorld::new(root, &command.font_paths);
    {
        if let Some((new_imgs, hashes)) = compile_once(&mut world, &command)? {
            *imgs.write().await = new_imgs;
            let conns = conns.clone();
            let imgs = imgs.clone();
            tokio::spawn(async move {
                broadcast_result(conns, &imgs.read().await, hashes).await;
            });
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
        .watch(&world.root, RecursiveMode::Recursive)
        .unwrap();

    // Handle events.
    info!("start watching files...");
    loop {
        let mut recompile = false;
        let mut events = vec![];
        while let Ok(e) =
            tokio::time::timeout(tokio::time::Duration::from_millis(100), rx.recv()).await
        {
            events.push(e);
        }
        for event in events.into_iter().flatten() {
            recompile |= world.relevant(&event);
        }
        if recompile {
            if let Some((new_imgs, hashes)) = compile_once(&mut world, &command)? {
                if !new_imgs.is_empty() {
                    {
                        *imgs.write().await = new_imgs;
                    }
                    let conns = conns.clone();
                    let imgs = imgs.clone();
                    tokio::spawn(async move {
                        broadcast_result(conns, &imgs.read().await, hashes).await;
                    });
                }
                comemo::evict(30);
            }
        }
    }
}

async fn broadcast_result(
    conns: Arc<Mutex<Vec<Arc<Client>>>>,
    imgs: &[tiny_skia::Pixmap],
    hashes: Vec<u128>,
) {
    let mut conn_lock = conns.lock().await;
    info!("render done, sending to {} clients", conn_lock.len());
    let mut to_be_remove: Vec<usize> = vec![];
    for (i, conn) in conn_lock.iter_mut().enumerate() {
        conn.update_hashes(hashes.clone()).await;
        if !conn.send(imgs).await {
            to_be_remove.push(i);
        }
    }
    // remove
    conn_lock.retain(with_index(|index, _item| !to_be_remove.contains(&index)));
}

fn hash_frame(page: &Frame) -> u128 {
    let mut hasher = SipHasher::new();
    page.hash(&mut hasher);
    hasher.finish128().as_u128()
}

/// Compile a single time.
fn compile_once(
    world: &mut SystemWorld,
    command: &CompileSettings,
) -> StrResult<Option<(Vec<tiny_skia::Pixmap>, Vec<u128>)>> {
    status(command, Status::Compiling).unwrap();

    world.reset();
    world.main = world
        .resolve(&command.input)
        .map_err(|err| err.to_string())?;

    match typst::compile(world) {
        // Export the images.
        Ok(document) => {
            let pixmaps: Vec<_> = document
                .pages
                .iter()
                .map(|frame| {
                    typst::export::render(
                        frame,
                        2.0,
                        typst::geom::Color::Rgba(RgbaColor::from_str("ffffff").unwrap()),
                    )
                })
                .collect();
            let hashes = document
                .pages
                .iter()
                .map(hash_frame)
                .collect();
            status(command, Status::Success).unwrap();
            Ok(Some((pixmaps, hashes)))
        }

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
