use std::path::PathBuf;

use clap::{ArgAction, Parser};

#[derive(Debug, Clone, Parser)]
#[clap(name = "typst-preview", author)]
pub struct CliArguments {
    /// Add additional directories to search for fonts
    #[clap(long = "font-path", value_name = "DIR", action = ArgAction::Append)]
    pub font_paths: Vec<PathBuf>,

    /// Configure the root for absolute paths
    #[clap(long = "root", value_name = "DIR", help = "root directory for your project")]
    pub root: Option<PathBuf>,

    /// Configure the websocket path
    #[clap(long = "data-plane-host", default_value = "127.0.0.1:23625", value_name = "HOST", help = "data plane server will bind to this address")]
    pub data_plane_host: String,

    /// Configure the websocket path
    #[clap(long = "control-plane-host", default_value = "127.0.0.1:23626", value_name = "HOST", help = "control plane server will bind to this address")]
    pub control_plane_host: Option<String>,

    #[clap(long = "open-in-browser-host", help = "Host to open the preview in the browser.", value_name = "HOST", default_value = "127.0.0.1:23267")]
    pub open_in_browser_host: Option<String>,

    #[clap(long = "open-in-browser", help = "Open the preview in the browser after compilation.")]
    pub open_in_browser: bool,

    #[clap(long = "partial-rendering", help = "Only render visible part of the document. This can improve performance but still being experimental.")]
    pub enable_partial_rendering: bool,
    
    pub input: PathBuf,
}

