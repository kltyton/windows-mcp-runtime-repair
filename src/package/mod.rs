mod cli;
mod discover;
mod ffi;

use std::path::PathBuf;

use anyhow::Result;

use crate::model::InstalledPackage;

pub(crate) struct Candidate {
    pub(crate) full_name: String,
    pub(crate) install_root: PathBuf,
    pub(crate) package_version: (u16, u16, u16, u16),
}

impl Candidate {
    fn resolve(self) -> Result<InstalledPackage> {
        let executable = self
            .install_root
            .join("app")
            .join("resources")
            .join("codex.exe");
        let backend_version = cli::backend_version(&executable)?;
        Ok(InstalledPackage {
            full_name: self.full_name,
            install_root: self.install_root,
            backend_version,
        })
    }
}

pub fn discover() -> Result<InstalledPackage> {
    match discover::launching_package()? {
        Some(candidate) => candidate.resolve(),
        None => discover::latest_installed(),
    }
}
