#![windows_subsystem = "windows"]

use anyhow::{Context, Result};
use mcp_runtime_repair::{
    bootstrap,
    installation::state::{self, RepairPaths},
    model::RepairInstallation,
    package, release,
};

fn run() -> Result<i32> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments
        .first()
        .is_some_and(|argument| argument == "--prepare-after-deployment")
    {
        let deployed = arguments
            .get(1)
            .context("Deployment event has no package name")?
            .to_str()
            .context("Deployment package name is not Unicode")?;
        // Other successful package deployments are discarded before any file or network work.
        if !deployed.starts_with("OpenAI.Codex_") || !deployed.ends_with("__2p2nqsd0c76g0") {
            return Ok(0);
        }
        if state::read::<RepairInstallation>("Installation")?.is_none() {
            return Ok(0);
        }
        let paths = RepairPaths::current_user()?;
        let package = package::discover()?;
        let runtime = release::prepare(&paths, &package, None)?;
        release::activate_plugins(&paths, &runtime)?;
        mcp_runtime_repair::installation::refresh_after_deployment(&paths, &package)?;
        return Ok(0);
    }
    bootstrap::run(arguments)
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            let message = format!("{error:#}");
            if let Ok(paths) = RepairPaths::current_user() {
                if let Err(log_error) =
                    std::fs::write(paths.root.join("last-update-error.log"), &message)
                {
                    eprintln!("Could not write runtime error: {log_error}");
                }
            }
            eprintln!("{message}");
            std::process::exit(1);
        }
    }
}
