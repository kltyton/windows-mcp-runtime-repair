pub mod environment;
pub mod state;
mod tasks;

use std::path::Path;

use anyhow::{Context, Result, ensure};

use crate::model::{InstalledPackage, InstalledRuntime, RepairInstallation};
use crate::release;
use state::RepairPaths;

pub fn install(
    paths: &RepairPaths,
    package: &InstalledPackage,
    catalog: Option<&Path>,
) -> Result<InstalledRuntime> {
    let runtime = release::prepare(paths, package, catalog)?;
    release::activate_plugins(paths, &runtime)?;
    let existing = state::read::<RepairInstallation>("Installation")?;
    let installation = match existing.clone() {
        Some(previous) => RepairInstallation {
            host_executable: paths.host(),
            ..previous
        },
        None => {
            let previous_cli = environment::read(environment::CLI_SETTING)?;
            let previous_plugins = environment::read(environment::PLUGIN_SETTING)?;
            let previous_cli_was_official = previous_cli
                .as_deref()
                .is_some_and(|value| Path::new(value) == package.official_cli());
            RepairInstallation {
                host_executable: paths.host(),
                previous_cli,
                previous_plugins,
                previous_cli_was_official,
            }
        }
    };
    let current = std::env::current_exe()?;
    let host = current
        .parent()
        .context("Repair executable has no containing directory")?
        .join("runtime-host.exe");
    if host != paths.host() {
        let same_host = paths.host().is_file()
            && release::payload::digest(&host)? == release::payload::digest(&paths.host())?;
        if !same_host {
            release::payload::plain_copy(&host, &paths.host())?;
        }
    }
    let previous_cli = environment::read(environment::CLI_SETTING)?;
    let previous_plugins = environment::read(environment::PLUGIN_SETTING)?;
    tasks::register(
        &installation.host_executable,
        &paths.temporary_root,
        existing
            .as_ref()
            .map(|value| value.host_executable.as_path()),
    )?;
    let commit = (|| -> Result<()> {
        environment::write(
            environment::CLI_SETTING,
            Some(
                installation
                    .host_executable
                    .to_str()
                    .context("Runtime host path is not Unicode")?,
            ),
        )?;
        environment::write(
            environment::PLUGIN_SETTING,
            Some(
                paths
                    .plugins()
                    .to_str()
                    .context("Plugin path is not Unicode")?,
            ),
        )?;
        state::write("Installation", &installation)
    })();
    if let Err(error) = commit {
        environment::write(environment::CLI_SETTING, previous_cli.as_deref())
            .context("Restore CLI setting after incomplete installation")?;
        environment::write(environment::PLUGIN_SETTING, previous_plugins.as_deref())
            .context("Restore plugin setting after incomplete installation")?;
        match existing {
            Some(previous) => tasks::register(
                &previous.host_executable,
                &paths.temporary_root,
                Some(&installation.host_executable),
            )?,
            None => tasks::unregister(&installation.host_executable)?,
        }
        return Err(error);
    }
    environment::notify();
    Ok(runtime)
}

pub fn uninstall(paths: &RepairPaths) -> Result<()> {
    let installation = state::read::<RepairInstallation>("Installation")?
        .context("This account has no repair installation")?;
    let current = environment::read(environment::CLI_SETTING)?;
    ensure!(
        current.as_deref().map(Path::new) == Some(installation.host_executable.as_path()),
        "Another backend now owns CODEX_CLI_PATH; settings were not changed"
    );
    tasks::unregister(&installation.host_executable)?;
    let previous_cli = if installation.previous_cli_was_official {
        None
    } else {
        installation.previous_cli.as_deref()
    };
    environment::write(environment::CLI_SETTING, previous_cli)?;
    if environment::read(environment::PLUGIN_SETTING)?
        .as_deref()
        .map(Path::new)
        == Some(paths.plugins().as_path())
    {
        environment::write(
            environment::PLUGIN_SETTING,
            installation.previous_plugins.as_deref(),
        )?;
    }
    state::remove("Installation")?;
    environment::notify();
    println!(
        "Removed the owned runtime hook. Cached runtimes at {} remain available for running processes.",
        paths.root.display()
    );
    Ok(())
}

pub fn refresh_after_deployment(paths: &RepairPaths, package: &InstalledPackage) -> Result<()> {
    let installation = state::read::<RepairInstallation>("Installation")?
        .context("The repair is no longer installed")?;
    let cli = environment::read(environment::CLI_SETTING)?;
    let plugins = environment::read(environment::PLUGIN_SETTING)?;
    let official = package.official_cli();
    ensure!(
        cli.as_deref()
            .map(Path::new)
            .is_none_or(|value| value == installation.host_executable || value == official),
        "A different backend owns the user setting; deployment did not replace it"
    );
    let generated_plugins = paths.plugins();
    let official_resources = package.resources();
    ensure!(
        plugins
            .as_deref()
            .map(Path::new)
            .is_none_or(|value| value == generated_plugins || value == official_resources),
        "A different plugin source owns the user setting; deployment did not replace it"
    );
    let host = installation
        .host_executable
        .to_str()
        .context("Runtime host path is not Unicode")?;
    let resource_root = generated_plugins
        .to_str()
        .context("Plugin path is not Unicode")?;
    if cli.as_deref() != Some(host) || plugins.as_deref() != Some(resource_root) {
        environment::write(environment::CLI_SETTING, Some(host))?;
        environment::write(environment::PLUGIN_SETTING, Some(resource_root))?;
        environment::notify();
    }
    Ok(())
}
