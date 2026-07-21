use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand, ValueEnum};

const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " ", env!("RATA_GIT_SHA"));

#[derive(Debug, Parser)]
#[command(name = "rata")]
#[command(about = "Context root discovery and scaffolding for AI agents")]
#[command(version = VERSION, disable_version_flag = true)]
pub struct Cli {
    /// Print the version and Git SHA.
    #[arg(
        short = 'v',
        long = "version",
        action = ArgAction::Version,
        required = false
    )]
    _version: Option<bool>,
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
        /// Choose which part of the resolved state to return.
        #[arg(value_enum, default_value_t = ResolveTarget::Summary)]
        target: ResolveTarget,
        /// Resolve relative to this directory instead of the current working directory.
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Override the global rata root for this invocation.
        #[arg(long)]
        global_root: Option<PathBuf>,
        /// Apply one or more additive context profiles in the order provided.
        #[arg(long = "profile")]
        profiles: Vec<String>,
        /// Choose human-readable or JSON output.
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Read the resolved context files and emit a deterministic context bundle.
    Pack {
        /// Resolve relative to this directory instead of the current working directory.
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Override the global rata root for this invocation.
        #[arg(long)]
        global_root: Option<PathBuf>,
        /// Apply one or more additive context profiles in the order provided.
        #[arg(long = "profile")]
        profiles: Vec<String>,
        /// Choose human-readable or JSON output.
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Read only a selected slice of the resolved context.
    Only {
        #[command(subcommand)]
        target: OnlyTarget,
        /// Resolve relative to this directory instead of the current working directory.
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Override the global rata root for this invocation.
        #[arg(long)]
        global_root: Option<PathBuf>,
        /// Choose human-readable or JSON output.
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Print built-in documentation for common rata workflows.
    Docs {
        #[arg(value_enum)]
        topic: DocsTopic,
    },
    /// Diagnose the active context stack.
    Doctor {
        #[command(subcommand)]
        target: Option<DoctorTarget>,
        /// Resolve relative to this directory instead of the current working directory.
        #[arg(long, global = true)]
        cwd: Option<PathBuf>,
        /// Override the global rata root for this invocation.
        #[arg(long, global = true)]
        global_root: Option<PathBuf>,
        /// Apply one or more additive context profiles in the order provided.
        #[arg(long = "profile", global = true)]
        profiles: Vec<String>,
        /// Choose human-readable or JSON output.
        #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Text)]
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum ResolveTarget {
    #[default]
    Summary,
    Stores,
}

#[derive(Debug, Subcommand)]
pub enum DoctorTarget {
    /// Show each store layer and its effective composition policy.
    Stores,
    /// Show effective settings and the settings contributed by every layer.
    Settings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum DocsTopic {
    Agent,
}

#[derive(Debug, Subcommand)]
pub enum OnlyTarget {
    /// Read only the files contributed by a named profile across all active scopes.
    Profile { name: String },
    /// Read only the files contributed by a scope kind.
    Scope {
        #[arg(value_enum)]
        kind: ScopeFilter,
    },
    /// Read only files whose basename matches the provided name across all active scopes.
    File { name: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ScopeFilter {
    Global,
    Local,
}
