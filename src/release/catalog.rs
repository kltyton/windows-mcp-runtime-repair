use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use reqwest::blocking::Client;
use semver::Version;

use crate::model::{CATALOG_URL, RuntimeCatalog, RuntimeRelease};

pub fn client() -> Result<Client> {
    Client::builder()
        .user_agent(concat!("mcp-runtime-repair/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .build()
        .context("Create update client using the configured system proxy")
}

pub fn load(local: Option<&Path>) -> Result<RuntimeCatalog> {
    let catalog: RuntimeCatalog = match local {
        Some(path) => serde_json::from_reader(std::fs::File::open(path)?)?,
        None => client()?
            .get(CATALOG_URL)
            .send()?
            .error_for_status()?
            .json()?,
    };
    ensure!(
        catalog.schema_version == 1,
        "Unsupported runtime catalog schema"
    );
    for runtime in &catalog.runtimes {
        validate_hash(&runtime.archive_sha256)?;
        validate_hash(&runtime.executable_sha256)?;
        ensure!(
            runtime.source_commit.len() == 40
                && runtime.source_commit.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid source commit in runtime catalog"
        );
        if local.is_none() {
            ensure!(
                runtime.archive_url.starts_with(
                    "https://github.com/kltyton/windows-mcp-runtime-repair/releases/download/"
                ),
                "Runtime asset is not hosted in the configured release repository"
            );
        }
    }
    Ok(catalog)
}

pub fn select(catalog: &RuntimeCatalog, version: &Version) -> Result<RuntimeRelease> {
    let matches: Vec<_> = catalog
        .runtimes
        .iter()
        .filter(|runtime| &runtime.backend_version == version)
        .collect();
    match matches.as_slice() {
        [runtime] => Ok((*runtime).clone()),
        [] => bail!(
            "A verified repair for backend {version} is not published yet; no older backend was substituted"
        ),
        _ => bail!("The catalog contains conflicting repairs for {version}"),
    }
}

pub fn validate_hash(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid SHA-256 in runtime catalog"
    );
    Ok(())
}
