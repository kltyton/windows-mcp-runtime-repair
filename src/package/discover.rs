use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;

use crate::model::{InstalledPackage, PACKAGE_FAMILY};

use super::{Candidate, ffi};

fn official_cli(install_root: &Path) -> PathBuf {
    install_root.join("app").join("resources").join("codex.exe")
}

pub(crate) fn candidate_from_full_name(full_name: &str) -> Result<Option<Candidate>> {
    let (architecture, package_version) = ffi::package_identity(full_name)?;
    if architecture != ffi::ARCH_AMD64 {
        return Ok(None);
    }
    let install_root = ffi::package_path(full_name)?;
    if !official_cli(&install_root).is_file() {
        return Ok(None);
    }
    Ok(Some(Candidate {
        full_name: full_name.to_owned(),
        install_root,
        package_version,
    }))
}

pub(crate) fn launching_package() -> Result<Option<Candidate>> {
    let parents = ffi::parent_map()?;
    // SAFETY: GetCurrentProcessId has no preconditions.
    let mut pid = unsafe { GetCurrentProcessId() };
    let mut visited = HashSet::new();
    while visited.insert(pid) {
        let Some(&parent) = parents.get(&pid) else {
            break;
        };
        if parent == 0 || parent == pid {
            break;
        }
        pid = parent;
        let Some(full_name) = ffi::process_package_full_name(pid)? else {
            continue;
        };
        let Some(family) = ffi::package_family_name(&full_name)? else {
            continue;
        };
        if !family.eq_ignore_ascii_case(PACKAGE_FAMILY) {
            continue;
        }
        if let Some(candidate) = candidate_from_full_name(&full_name)? {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

pub(crate) fn latest_installed() -> Result<InstalledPackage> {
    let mut candidates = Vec::new();
    for full_name in ffi::family_packages(PACKAGE_FAMILY)? {
        if let Some(candidate) = candidate_from_full_name(&full_name)? {
            candidates.push(candidate);
        }
    }
    candidates.sort_by(|left, right| right.package_version.cmp(&left.package_version));
    match candidates.into_iter().next() {
        Some(candidate) => candidate
            .resolve()
            .context("failed to read the backend version from the newest package"),
        None => bail!("no x64 {PACKAGE_FAMILY} package with app/resources/codex.exe was found"),
    }
}
