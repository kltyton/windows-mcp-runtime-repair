use std::ffi::OsString;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

use crate::installation::state::RepairPaths;
use crate::{package, release};

pub fn run(arguments: Vec<OsString>) -> Result<i32> {
    let paths = RepairPaths::current_user()?;
    let package = package::discover()?;
    let runtime = release::prepare(&paths, &package, None)?;
    release::activate_plugins(&paths, &runtime)?;
    let status = Command::new(&runtime.executable)
        .args(arguments)
        .env("CODEX_CLI_PATH", &runtime.executable)
        .env(
            "CODEX_ELECTRON_BUNDLED_PLUGINS_RESOURCES_PATH",
            paths.plugins(),
        )
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .creation_flags(0x0800_0000)
        .status()
        .context("Start the version-matched desktop backend")?;
    status
        .code()
        .context("Backend exited without a Windows exit status")
}
