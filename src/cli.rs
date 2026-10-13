use clap::{Parser, Subcommand};

/// Minimalist, dumb and fast wayland native clipboard history manager
///
/// Supports listing and picking a clipboard history items with an interactive user interface or the cli
#[derive(Parser, Debug)]
#[command(about, long_about=None)]
pub struct Cli {
    #[arg(short)]
    pub verbose: bool,

    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand, Default, Debug)]
pub enum Cmd {
    #[default]
    /// Spawn a wayland client allowing for a clipboard choice
    Ui,
    /// Start klemmbrett in daemon mode
    Daemon,
    /// Choose clipboard history item from the history via cli
    Pick { element: usize },
    /// List available clipboard history items
    List,
}
