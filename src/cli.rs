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
    /// Read one node: its body plus the signatures of its children.
    Show {
        /// A ref: `memory:some-note`, `AGENTS.md#Safety`, or `workflow/sdlc.md#Phases/PR summaries`.
        reference: String,
        /// Descend this many levels, including their bodies. 0 lists children as signatures only.
        #[arg(long, default_value_t = 0)]
        depth: usize,
        /// Resolve relative to this directory instead of the current working directory.
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Override the global rata root for this invocation.
        #[arg(long)]
        global_root: Option<PathBuf>,
        /// Apply one or more additive context profiles, widening the ref space.
        #[arg(long = "profile")]
        profiles: Vec<String>,
        /// Choose human-readable or JSON output.
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Render a computed index: a store's nodes, or one file's heading tree.
    Outline {
        /// A store name, or a file ref to outline its headings. Defaults to every resolved store.
        target: Option<String>,
        /// Cap how deep to descend: store subdirectories, or heading levels for a file ref.
        #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
        depth: Option<u16>,
        /// Resolve relative to this directory instead of the current working directory.
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Override the global rata root for this invocation.
        #[arg(long)]
        global_root: Option<PathBuf>,
        /// Apply one or more additive context profiles, widening the ref space.
        #[arg(long = "profile")]
        profiles: Vec<String>,
        /// Choose human-readable or JSON output.
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Find every node that links to a ref — the reverse of `show`.
    Callers {
        /// A ref: `context/PREFERENCES.md`, `AGENTS.md#Safety`, `memory:some-note`.
        reference: String,
        /// Resolve relative to this directory instead of the current working directory.
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Override the global rata root for this invocation.
        #[arg(long)]
        global_root: Option<PathBuf>,
        /// Apply one or more additive context profiles, widening the graph.
        #[arg(long = "profile")]
        profiles: Vec<String>,
        /// Choose human-readable or JSON output.
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Render the link graph. Rendering only — no layout opinions.
    Graph {
        /// Diagram syntax to emit as text.
        #[arg(long, value_enum, default_value_t = GraphSyntax::Mermaid)]
        syntax: GraphSyntax,
        /// Choose the rendered diagram or the raw graph as JSON.
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
        /// Restrict to what is reachable from this ref.
        #[arg(long)]
        from: Option<String>,
        /// Follow at most this many hops from `--from`.
        #[arg(long)]
        depth: Option<usize>,
        /// Resolve relative to this directory instead of the current working directory.
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Override the global rata root for this invocation.
        #[arg(long)]
        global_root: Option<PathBuf>,
        /// Apply one or more additive context profiles, widening the graph.
        #[arg(long = "profile")]
        profiles: Vec<String>,
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
pub enum GraphSyntax {
    #[default]
    Mermaid,
    Dot,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum ResolveTarget {
    #[default]
    Summary,
    Stores,
}

#[derive(Debug, Subcommand)]
pub enum DoctorTarget {
    /// Show every store node, which signature ladder tier it resolved at, and its frontmatter issues.
    Nodes {
        /// Limit the report to a single store.
        store: Option<String>,
    },
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
