use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use installation::state::{self, RepairPaths};
use mcp_runtime_repair::{installation, model::RepairInstallation, package, release};

#[derive(Parser)]
#[command(version, about = "Version-aware Windows MCP runtime repair")]
struct Arguments {
    #[arg(long, global = true)]
    temporary_directory: Option<PathBuf>,
    #[command(subcommand)]
    command: Operation,
}

#[derive(Subcommand)]
enum Operation {
    Install {
        #[arg(long)]
        catalog_file: Option<PathBuf>,
    },
    Prepare {
        #[arg(long)]
        catalog_file: Option<PathBuf>,
    },
    Status,
    Uninstall,
}

fn run() -> Result<()> {
    let arguments = Arguments::parse();
    if let Some(directory) = arguments.temporary_directory {
        state::write("TemporaryRoot", &directory)?;
    }
    let paths = RepairPaths::current_user()?;
    match arguments.command {
        Operation::Install { catalog_file } => {
            let package = package::discover()?;
            let runtime = installation::install(&paths, &package, catalog_file.as_deref())?;
            println!(
                "Installed matching backend {}. Fully exit and reopen the original application to load it.",
                runtime.backend_version
            );
        }
        Operation::Prepare { catalog_file } => {
            let package = package::discover()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&release::prepare(
                    &paths,
                    &package,
                    catalog_file.as_deref()
                )?)?
            );
        }
        Operation::Status => {
            let package = package::discover()?;
            let installation = state::read::<RepairInstallation>("Installation")?;
            let runtime = release::cached(&paths, &package)?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"package":package,"installation":installation,"runtime":runtime,"cli_path":installation::environment::read(installation::environment::CLI_SETTING)?})
                )?
            );
        }
        Operation::Uninstall => installation::uninstall(&paths)?,
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
