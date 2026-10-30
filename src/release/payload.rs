use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::model::RuntimeRelease;

use super::catalog;

pub fn digest(path: &Path) -> Result<String> {
    let mut input = File::open(path)?;
    let mut hash = Sha256::new();
    let mut bytes = [0_u8; 128 * 1024];
    loop {
        let count = input.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
    }
    Ok(hex::encode(hash.finalize()))
}

pub fn plain_copy(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .context("Runtime file has no parent directory")?;
    fs::create_dir_all(parent)?;
    let mut input = File::open(source)?;
    let mut output = File::create(destination)?;
    std::io::copy(&mut input, &mut output)?;
    output.flush()?;
    output.sync_all()?;
    Ok(())
}

pub fn mirror_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(
            !kind.is_symlink(),
            "A plugin source contains an unexpected link"
        );
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            mirror_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            plain_copy(&entry.path(), &target)?;
        }
    }
    Ok(())
}

pub fn extract(release: &RuntimeRelease, destination: &Path, temporary_root: &Path) -> Result<()> {
    fs::create_dir_all(temporary_root)?;
    let mut download = tempfile::NamedTempFile::new_in(temporary_root)?;
    let output = download.as_file_mut();
    if let Some(source) = release.archive_url.strip_prefix("file://") {
        std::io::copy(&mut File::open(source)?, output)?;
    } else {
        let mut response = catalog::client()?
            .get(&release.archive_url)
            .send()?
            .error_for_status()?;
        std::io::copy(&mut response, output)?;
    }
    output.flush()?;
    output.sync_all()?;
    ensure!(
        digest(download.path())?.eq_ignore_ascii_case(&release.archive_sha256),
        "The runtime archive checksum does not match its release"
    );
    let mut archive = ZipArchive::new(File::open(download.path())?)?;
    fs::create_dir_all(destination)?;
    for name in ["codex.exe", "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.txt"] {
        let mut input = archive
            .by_name(name)
            .with_context(|| format!("Runtime archive is missing {name}"))?;
        let mut file = File::create(destination.join(name))?;
        std::io::copy(&mut input, &mut file)?;
        file.sync_all()?;
    }
    ensure!(
        digest(&destination.join("codex.exe"))?.eq_ignore_ascii_case(&release.executable_sha256),
        "The extracted runtime does not match the verified executable"
    );
    drop(archive);
    download
        .close()
        .context("Remove completed update download")?;
    Ok(())
}
