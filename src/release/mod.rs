pub mod catalog;
pub mod payload;

use std::fs::{self, File};
use std::os::windows::fs::MetadataExt;
use std::path::Path;

use anyhow::{Context, Result, ensure};

use crate::installation::state::{self, RepairPaths};
use crate::model::{InstalledPackage, InstalledRuntime};

fn registration(package: &InstalledPackage) -> String {
    format!("Runtime-{}", package.full_name)
}

pub fn cached(paths: &RepairPaths, package: &InstalledPackage) -> Result<Option<InstalledRuntime>> {
    let runtime = state::read::<InstalledRuntime>(&registration(package))?;
    match runtime {
        Some(runtime)
            if runtime.package_full_name == package.full_name
                && runtime.backend_version == package.backend_version
                && runtime.executable.is_file() =>
        {
            let executable = runtime.executable.canonicalize()?;
            let root = paths.root.canonicalize()?;
            ensure!(
                executable.starts_with(root),
                "Registered runtime is outside the owned installation"
            );
            ensure!(
                executable.metadata()?.len() == runtime.executable_bytes,
                "Registered runtime size changed; prepare the package again"
            );
            Ok(Some(runtime))
        }
        Some(_) | None => Ok(None),
    }
}

pub fn prepare(
    paths: &RepairPaths,
    package: &InstalledPackage,
    local_catalog: Option<&Path>,
) -> Result<InstalledRuntime> {
    fs::create_dir_all(&paths.root)?;
    // Policy and Work bootstrap can start together; coordinate only package preparation.
    let lock = File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(
            paths
                .root
                .join(format!("{}.prepare.lock", package.full_name)),
        )?;
    lock.lock()
        .context("Wait for this package's existing preparation")?;
    if let Some(runtime) = cached(paths, package)? {
        return Ok(runtime);
    }
    let catalog = catalog::load(local_catalog)?;
    let release = catalog::select(&catalog, &package.backend_version)?;
    let destination = paths
        .root
        .join("runtimes")
        .join(&package.full_name)
        .join(&release.executable_sha256);
    payload::extract(&release, &destination, &paths.temporary_root)?;
    for entry in fs::read_dir(package.resources())? {
        let entry = entry?;
        let extension = entry
            .path()
            .extension()
            .map(|value| value.to_string_lossy().to_ascii_lowercase());
        if entry.file_type()?.is_file()
            && entry.file_name() != "codex.exe"
            && matches!(extension.as_deref(), Some("exe" | "dll"))
        {
            payload::plain_copy(&entry.path(), &destination.join(entry.file_name()))?;
        }
    }
    let plugin_resources = paths.root.join("resources").join(&package.full_name);
    payload::mirror_tree(
        &package.resources().join("plugins"),
        &plugin_resources.join("plugins"),
    )?;
    let executable = destination.join("codex.exe");
    let executable_bytes = executable.metadata()?.len();
    let runtime = InstalledRuntime {
        package_full_name: package.full_name.clone(),
        backend_version: package.backend_version.clone(),
        executable,
        executable_sha256: release.executable_sha256,
        plugin_resources,
        executable_bytes,
    };
    state::write(&registration(package), &runtime)?;
    Ok(runtime)
}

pub fn activate_plugins(paths: &RepairPaths, runtime: &InstalledRuntime) -> Result<()> {
    let lock = File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(paths.root.join("plugins.activate.lock"))?;
    lock.lock()
        .context("Wait for the current plugin publication")?;
    let active = state::read::<String>("ActivePluginPackage")?;
    if active.as_deref() == Some(&runtime.package_full_name)
        && paths.plugins().join("plugins").is_dir()
    {
        return Ok(());
    }
    exact_mirror(
        &runtime.plugin_resources.join("plugins"),
        &paths.plugins().join("plugins"),
    )?;
    state::write("ActivePluginPackage", &runtime.package_full_name)
}

fn exact_mirror(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    let wanted = fs::read_dir(source)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    for entry in fs::read_dir(destination)? {
        let entry = entry?;
        ensure!(
            entry.metadata()?.file_attributes() & 0x400 == 0,
            "Generated plugin mirror contains an unexpected reparse point"
        );
        if !wanted.contains(&entry.file_name()) {
            remove_generated(&entry.path(), destination)?;
        }
    }
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        ensure!(
            entry.metadata()?.file_attributes() & 0x400 == 0,
            "Plugin generation contains an unexpected reparse point"
        );
        let target = destination.join(entry.file_name());
        if let Ok(metadata) = fs::symlink_metadata(&target) {
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "Generated plugin target contains an unexpected reparse point"
            );
            if metadata.is_dir() != entry.file_type()?.is_dir() {
                remove_generated(&target, destination)?;
            }
        }
        if entry.file_type()?.is_dir() {
            exact_mirror(&entry.path(), &destination.join(entry.file_name()))?;
        } else {
            payload::plain_copy(&entry.path(), &destination.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn remove_generated(path: &Path, owner: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_attributes() & 0x400 == 0,
        "Generated plugin cleanup encountered a reparse point"
    );
    let absolute = path.canonicalize()?;
    ensure!(
        absolute.starts_with(owner.canonicalize()?) && absolute != owner.canonicalize()?,
        "Generated plugin cleanup escaped its owned directory"
    );
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            remove_generated(&entry?.path(), owner)?;
        }
        fs::remove_dir(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}
