use std::path::PathBuf;

use clap::{ArgAction, Parser};

#[derive(Debug, Clone, Parser)]
#[clap(name = "typst-preview", author)]
pub struct CliArguments {
    /// Add additional directories to search for fonts
    #[clap(long = "font-path", value_name = "DIR", action = ArgAction::Append)]
    pub font_paths: Vec<PathBuf>,

    /// Root directory for your project
    #[clap(long = "root", value_name = "DIR")]
    pub root: Option<PathBuf>,

    /// Data plane server will bind to this address
    #[clap(
        long = "data-plane-host",
        default_value = "127.0.0.1:23625",
        value_name = "HOST"
    )]
    pub data_plane_host: String,

    /// Control plane server will bind to this address
    #[clap(
        long = "control-plane-host",
        default_value = "127.0.0.1:23626",
        value_name = "HOST"
    )]
    pub control_plane_host: String,

    /// Host to open the preview in the browser.
    #[clap(
        long = "static-file-host",
        value_name = "HOST",
        default_value = "127.0.0.1:23627"
    )]
    pub static_file_host: String,

    /// Open the preview in the browser after compilation.
    #[clap(long = "open-in-browser")]
    pub open_in_browser: bool,

    /// Serve html for preview in the browser.
    #[clap(long = "server-static-file")]
    pub server_static_file: bool,

    /// Only render visible part of the document. This can improve performance but still being experimental.
    #[clap(long = "partial-rendering")]
    pub enable_partial_rendering: bool,

    pub input: PathBuf,
}
