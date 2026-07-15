mod cli;
mod config;
mod errors;
mod init;
mod resolve;

use std::path::PathBuf;

use clap::Parser;

use crate::cli::{Cli, Commands, InitScope, OutputFormat};
use crate::errors::Result;

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { scope, root } => {
            let root = root.unwrap_or_else(|| default_root_for_init(scope));
            init::scaffold(scope, &root)?;
            println!("initialized {} root at {}", scope.label(), root.display());
        }
        Commands::Resolve { cwd, format } => {
            let cwd = cwd.unwrap_or(std::env::current_dir()?);
            let manifest = resolve::resolve_manifest(&cwd)?;
            match format {
                OutputFormat::Text => print!("{manifest}"),
                OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&manifest)?),
            }
        }
    }

    Ok(())
}

fn default_root_for_init(scope: InitScope) -> PathBuf {
    match scope {
        InitScope::Global => config::default_global_root(),
        InitScope::Local => std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".ratatoskr"),
    }
}
