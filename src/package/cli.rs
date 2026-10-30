use std::{os::windows::process::CommandExt, path::Path, process::Command};

use anyhow::{Context, Result, bail};
use semver::Version;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(crate) fn backend_version(executable: &Path) -> Result<Version> {
    let output = Command::new(executable)
        .arg("--version")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .with_context(|| format!("failed to run {} --version", executable.display()))?;
    if !output.status.success() {
        bail!(
            "{} --version exited with {}",
            executable.display(),
            output.status
        );
    }
    let text = String::from_utf8(output.stdout).context("Backend version output is not UTF-8")?;
    let reported = text
        .trim()
        .strip_prefix("codex-cli ")
        .context("Official backend did not report the expected version format")?;
    Version::parse(reported.trim()).context("Official backend reported an invalid semantic version")
}
