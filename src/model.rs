use std::path::PathBuf;

use semver::Version;
use serde::{Deserialize, Serialize};

pub const PACKAGE_FAMILY: &str = "OpenAI.Codex_2p2nqsd0c76g0";
pub const CATALOG_URL: &str = "https://raw.githubusercontent.com/kltyton/windows-mcp-runtime-repair/main/runtime-catalog.json";

#[derive(Clone, Debug, Serialize)]
pub struct InstalledPackage {
    pub full_name: String,
    pub install_root: PathBuf,
    pub backend_version: Version,
}

impl InstalledPackage {
    pub fn resources(&self) -> PathBuf {
        self.install_root.join("app").join("resources")
    }

    pub fn official_cli(&self) -> PathBuf {
        self.resources().join("codex.exe")
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RuntimeCatalog {
    pub schema_version: u32,
    pub runtimes: Vec<RuntimeRelease>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RuntimeRelease {
    pub backend_version: Version,
    pub source_commit: String,
    pub archive_url: String,
    pub archive_sha256: String,
    pub executable_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InstalledRuntime {
    pub package_full_name: String,
    pub backend_version: Version,
    pub executable: PathBuf,
    pub executable_sha256: String,
    pub plugin_resources: PathBuf,
    pub executable_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RepairInstallation {
    pub host_executable: PathBuf,
    pub previous_cli: Option<String>,
    pub previous_plugins: Option<String>,
    pub previous_cli_was_official: bool,
}
