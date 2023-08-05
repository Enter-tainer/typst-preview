use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand};

/// typst creates PDF files from .typ files
#[derive(Debug, Clone, Parser)]
#[clap(name = "typst-ws", author)]
pub struct CliArguments {
    /// Add additional directories to search for fonts
    #[clap(long = "font-path", value_name = "DIR", action = ArgAction::Append)]
    pub font_paths: Vec<PathBuf>,

    /// Configure the root for absolute paths
    #[clap(long = "root", value_name = "DIR")]
    pub root: Option<PathBuf>,

    /// Configure the websocket path
    #[clap(long = "data-plane-host")]
    pub data_plane_host: Option<String>,

    /// Configure the websocket path
    #[clap(long = "control-plane-host")]
    pub control_plane_host: Option<String>,

    #[clap(long = "static-file-host")]
    pub static_file_host: Option<String>,

    #[clap(long = "static-file-path")]
    pub static_file_path: Option<String>,

    #[clap(long = "partial-rendering")]
    pub enable_partial_rendering: bool,

    /// The typst command to run
    #[command(subcommand)]
    pub command: Command,
}

/// What to do.
#[derive(Debug, Clone, Subcommand)]
#[command()]
pub enum Command {
    /// Watches the input file and recompiles on changes
    #[command(visible_alias = "w")]
    Watch(CompileCommand),
}

/// Compiles the input file into a PDF file
#[derive(Debug, Clone, Parser)]
pub struct CompileCommand {
    /// Path to input Typst file
    pub input: PathBuf,
}
