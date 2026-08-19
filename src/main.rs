mod cli;
mod config;
mod docs;
mod doctor;
mod errors;
mod frontmatter;
mod init;
mod outline;
mod pack;
mod resolve;
#[cfg(test)]
mod test_support;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, Commands, DoctorTarget, InitScope, OutputFormat, ResolveTarget};
use crate::errors::Result;

/// `doctor` exits non-zero when it finds problems, so a hook or CI step can gate on it.
const UNHEALTHY: u8 = 2;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { scope, root } => {
            let root = root.unwrap_or_else(|| default_root_for_init(scope));
            init::scaffold(scope, &root)?;
            println!("initialized {} root at {}", scope.label(), root.display());
        }
        Commands::Resolve {
            target,
            cwd,
            global_root,
            profiles,
            format,
        } => {
            let cwd = cwd.unwrap_or(std::env::current_dir()?);
            match target {
                ResolveTarget::Summary => {
                    let manifest =
                        resolve::resolve_manifest(&cwd, global_root.as_deref(), &profiles)?;
                    match format {
                        OutputFormat::Text => print!("{manifest}"),
                        OutputFormat::Json => {
                            println!("{}", serde_json::to_string_pretty(&manifest)?)
                        }
                    }
                }
                ResolveTarget::Stores => {
                    let stores = resolve::resolve_stores(&cwd, global_root.as_deref(), &profiles)?;
                    match format {
                        OutputFormat::Text => print!("{stores}"),
                        OutputFormat::Json => {
                            println!("{}", serde_json::to_string_pretty(&stores)?)
                        }
                    }
                }
            }
        }
        Commands::Pack {
            cwd,
            global_root,
            profiles,
            format,
        } => {
            let cwd = cwd.unwrap_or(std::env::current_dir()?);
            let bundle = pack::build_bundle(&cwd, global_root.as_deref(), &profiles)?;
            match format {
                OutputFormat::Text => print!("{bundle}"),
                OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&bundle)?),
            }
        }
        Commands::Only {
            target,
            cwd,
            global_root,
            format,
        } => {
            let cwd = cwd.unwrap_or(std::env::current_dir()?);
            let bundle = pack::build_only_bundle(&cwd, global_root.as_deref(), &target)?;
            match format {
                OutputFormat::Text => print!("{bundle}"),
                OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&bundle)?),
            }
        }
        Commands::Outline {
            store,
            depth,
            cwd,
            global_root,
            format,
        } => {
            let cwd = cwd.unwrap_or(std::env::current_dir()?);
            let report =
                outline::build_outline(&cwd, global_root.as_deref(), store.as_deref(), depth)?;
            match format {
                OutputFormat::Text => print!("{report}"),
                OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
            }
        }
        Commands::Docs { topic } => {
            print!("{}", docs::render(topic));
        }
        Commands::Doctor {
            target,
            cwd,
            global_root,
            profiles,
            format,
        } => {
            let cwd = cwd.unwrap_or(std::env::current_dir()?);
            match target {
                None => {
                    let report = doctor::run_doctor(&cwd, global_root.as_deref(), &profiles)?;
                    match format {
                        OutputFormat::Text => print!("{report}"),
                        OutputFormat::Json => {
                            println!("{}", serde_json::to_string_pretty(&report)?)
                        }
                    }
                    if !report.healthy {
                        return Ok(ExitCode::from(UNHEALTHY));
                    }
                }
                Some(DoctorTarget::Nodes { store }) => {
                    let report =
                        doctor::run_nodes_doctor(&cwd, global_root.as_deref(), store.as_deref())?;
                    match format {
                        OutputFormat::Text => print!("{report}"),
                        OutputFormat::Json => {
                            println!("{}", serde_json::to_string_pretty(&report)?)
                        }
                    }
                    if !report.healthy {
                        return Ok(ExitCode::from(UNHEALTHY));
                    }
                }
                Some(DoctorTarget::Stores) => {
                    let report =
                        doctor::run_stores_doctor(&cwd, global_root.as_deref(), &profiles)?;
                    match format {
                        OutputFormat::Text => print!("{report}"),
                        OutputFormat::Json => {
                            println!("{}", serde_json::to_string_pretty(&report)?)
                        }
                    }
                }
                Some(DoctorTarget::Settings) => {
                    let report =
                        doctor::run_settings_doctor(&cwd, global_root.as_deref(), &profiles)?;
                    match format {
                        OutputFormat::Text => print!("{report}"),
                        OutputFormat::Json => {
                            println!("{}", serde_json::to_string_pretty(&report)?)
                        }
                    }
                }
            }
        }
    }

    Ok(ExitCode::SUCCESS)
}

fn default_root_for_init(scope: InitScope) -> PathBuf {
    match scope {
        InitScope::Global => config::default_global_root(),
        InitScope::Local => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    }
}
