use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "rata")]
#[command(about = "Context root discovery and scaffolding for AI agents")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Scaffold a global or local ratatoskr root.
    Init {
        #[arg(value_enum)]
        scope: InitScope,
        /// Override the target root path.
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Resolve the active global/local context stack.
    Resolve {
        /// Resolve relative to this directory instead of the current working directory.
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Choose human-readable or JSON output.
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum InitScope {
    Global,
    Local,
}

impl InitScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Local => "local",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}
