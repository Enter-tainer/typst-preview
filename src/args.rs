use std::path::PathBuf;

use clap::{ArgAction, Parser, ValueEnum};
use once_cell::sync::Lazy;

// enum Preview Mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PreviewMode {
    /// Preview mode for regular document
    #[clap(name = "document")]
    Document,

    /// Preview mode for slide
    #[clap(name = "slide")]
    Slide,
}

#[derive(Debug, Clone, Parser)]
#[clap(name = "typst-preview", author, version, about, long_version(LONG_VERSION.as_str()))]
pub struct CliArguments {
    /// Add additional directories to search for fonts
    #[clap(long = "font-path", value_name = "DIR", action = ArgAction::Append, env = "TYPST_FONT_PATHS")]
    pub font_paths: Vec<PathBuf>,

    /// Root directory for your project
    #[clap(long = "root", value_name = "DIR")]
    pub root: Option<PathBuf>,

    /// Preview mode
    #[clap(long = "preview-mode", default_value = "document", value_name = "MODE")]
    pub preview_mode: PreviewMode,

    /// Data plane server will bind to this address
    #[clap(
        long = "data-plane-host",
        default_value = "127.0.0.1:23625",
        value_name = "HOST",
        hide(true)
    )]
    pub data_plane_host: String,

    /// Control plane server will bind to this address
    #[clap(
        long = "control-plane-host",
        default_value = "127.0.0.1:23626",
        value_name = "HOST",
        hide(true)
    )]
    pub control_plane_host: String,

    /// Host for the preview server
    #[clap(
        long = "host",
        value_name = "HOST",
        default_value = "127.0.0.1:23627",
        alias = "static-file-host"
    )]
    pub static_file_host: String,

    /// Don't open the preview in the browser after compilation.
    #[clap(long = "no-open")]
    pub dont_open_in_browser: bool,

    /// Only render visible part of the document. This can improve performance but still being experimental.
    #[clap(long = "partial-rendering")]
    pub enable_partial_rendering: bool,

    pub input: PathBuf,
}

static NONE: &str = "None";
static LONG_VERSION: Lazy<String> = Lazy::new(|| {
    format!(
        "
Build Timestamp:     {}
Build Git Describe:  {}
Commit SHA:          {}
Commit Date:         {}
Commit Branch:       {}
Cargo Target Triple: {}
",
        env!("VERGEN_BUILD_TIMESTAMP"),
        env!("VERGEN_GIT_DESCRIBE"),
        option_env!("VERGEN_GIT_SHA").unwrap_or(NONE),
        option_env!("VERGEN_GIT_COMMIT_TIMESTAMP").unwrap_or(NONE),
        option_env!("VERGEN_GIT_BRANCH").unwrap_or(NONE),
        env!("VERGEN_CARGO_TARGET_TRIPLE"),
    )
});
