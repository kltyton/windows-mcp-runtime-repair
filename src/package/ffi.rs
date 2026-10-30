use std::{collections::HashMap, mem::size_of, path::PathBuf, ptr};

use anyhow::{Context, Result, bail};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_FILES, ERROR_SUCCESS, HANDLE,
        INVALID_HANDLE_VALUE,
    },
    Storage::Packaging::Appx::{
        GetPackageFullName, GetPackagePathByFullName, GetPackagesByPackageFamily, PACKAGE_ID,
        PACKAGE_INFORMATION_BASIC, PackageFamilyNameFromFullName, PackageIdFromFullName,
    },
    System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    },
    System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
};

/// PROCESSOR_ARCHITECTURE_AMD64 from winnt.h; the windows-sys constant lives
/// in a feature this crate deliberately does not enable.
pub(crate) const ARCH_AMD64: u32 = 9;

pub(crate) type PackageVersion = (u16, u16, u16, u16);

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            // SAFETY: the handle was returned by a Win32 call and is owned here.
            if unsafe { CloseHandle(self.0) } == 0 {
                eprintln!(
                    "Could not close package-query handle: {}",
                    std::io::Error::last_os_error()
                );
            }
        }
    }
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buffer: &[u16]) -> Result<String> {
    let end = buffer
        .iter()
        .position(|&value| value == 0)
        .context("Windows package string has no terminator inside its buffer")?;
    String::from_utf16(&buffer[..end]).context("Windows package string is not valid UTF-16")
}

fn query_string(mut call: impl FnMut(*mut u32, *mut u16) -> u32) -> Result<Option<String>> {
    let mut length = 0u32;
    let status = call(&mut length, ptr::null_mut());
    if status == 15700 {
        return Ok(None);
    }
    if status != ERROR_INSUFFICIENT_BUFFER {
        bail!("Windows package string size query failed with error {status}");
    }
    let mut buffer = vec![0u16; usize::try_from(length)?];
    let status = call(&mut length, buffer.as_mut_ptr());
    if status != ERROR_SUCCESS {
        bail!("Windows package string query failed with error {status}");
    }
    let written = buffer
        .get(..usize::try_from(length)?)
        .context("Windows package string exceeds its allocated buffer")?;
    Ok(Some(from_wide(written)?))
}

pub(crate) fn parent_map() -> Result<HashMap<u32, u32>> {
    // SAFETY: no pointers are passed; the returned handle is checked and guarded.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot.is_null() || snapshot == INVALID_HANDLE_VALUE {
        bail!(
            "CreateToolhelp32Snapshot failed: {}",
            std::io::Error::last_os_error()
        );
    }
    let snapshot = OwnedHandle(snapshot);
    let mut entry = PROCESSENTRY32W {
        dwSize: u32::try_from(size_of::<PROCESSENTRY32W>())?,
        ..PROCESSENTRY32W::default()
    };
    // SAFETY: `entry` is initialized with its required dwSize and lives for the call.
    if unsafe { Process32FirstW(snapshot.0, &mut entry) } == 0 {
        bail!(
            "Process32FirstW failed: {}",
            std::io::Error::last_os_error()
        );
    }
    let mut parents = HashMap::new();
    loop {
        parents.insert(entry.th32ProcessID, entry.th32ParentProcessID);
        // SAFETY: the snapshot handle is valid and `entry` remains writable.
        if unsafe { Process32NextW(snapshot.0, &mut entry) } == 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(i32::try_from(ERROR_NO_MORE_FILES)?) {
                return Err(error).context("Enumerate package ancestors");
            }
            break;
        }
    }
    Ok(parents)
}

pub(crate) fn process_package_full_name(pid: u32) -> Result<Option<String>> {
    // SAFETY: the access mask is documented for package identity queries.
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if raw.is_null() {
        let error = std::io::Error::last_os_error();
        if matches!(error.raw_os_error(), Some(5 | 87)) {
            return Ok(None);
        }
        return Err(error).context("Open package ancestor for identity query");
    }
    let handle = OwnedHandle(raw);
    // SAFETY: `handle` stays alive while the API writes into the temporary buffer.
    query_string(|length, buffer| unsafe { GetPackageFullName(handle.0, length, buffer) })
}

pub(crate) fn package_family_name(full_name: &str) -> Result<Option<String>> {
    let full_name = to_wide(full_name);
    // SAFETY: the wide string is null-terminated and outlives the call.
    query_string(|length, buffer| unsafe {
        PackageFamilyNameFromFullName(full_name.as_ptr(), length, buffer)
    })
}

pub(crate) fn package_path(full_name: &str) -> Result<PathBuf> {
    let wide = to_wide(full_name);
    // SAFETY: the wide string is null-terminated and outlives the call.
    let path = query_string(|length, buffer| unsafe {
        GetPackagePathByFullName(wide.as_ptr(), length, buffer)
    })?;
    match path {
        Some(path) if !path.is_empty() => Ok(PathBuf::from(path)),
        _ => bail!("package path is unavailable for {full_name}"),
    }
}

pub(crate) fn package_identity(full_name: &str) -> Result<(u32, PackageVersion)> {
    let wide = to_wide(full_name);
    let mut length = 0u32;
    // SAFETY: a null buffer requests the required byte length.
    let status = unsafe {
        PackageIdFromFullName(
            wide.as_ptr(),
            PACKAGE_INFORMATION_BASIC,
            &mut length,
            ptr::null_mut(),
        )
    };
    if status != ERROR_INSUFFICIENT_BUFFER {
        bail!("PackageIdFromFullName size query failed for {full_name}: {status}");
    }
    let byte_length = usize::try_from(length)?;
    anyhow::ensure!(
        byte_length >= size_of::<PACKAGE_ID>(),
        "Windows package identity buffer is too short"
    );
    let mut buffer = vec![0usize; byte_length.div_ceil(size_of::<usize>())];
    // SAFETY: the buffer has the byte length reported by the previous call.
    let status = unsafe {
        PackageIdFromFullName(
            wide.as_ptr(),
            PACKAGE_INFORMATION_BASIC,
            &mut length,
            buffer.as_mut_ptr().cast::<u8>(),
        )
    };
    if status != ERROR_SUCCESS {
        bail!("PackageIdFromFullName failed for {full_name}: {status}");
    }
    // SAFETY: the successful API initialized PACKAGE_ID in a usize-aligned allocation
    // whose checked byte length covers the complete structure.
    let package_id = unsafe { ptr::read(buffer.as_ptr().cast::<PACKAGE_ID>()) };
    // SAFETY: PACKAGE_ID.version is a union populated by the successful API call.
    let version = unsafe { package_id.version.Anonymous.Anonymous };
    Ok((
        package_id.processorArchitecture,
        (
            version.Major,
            version.Minor,
            version.Build,
            version.Revision,
        ),
    ))
}

pub(crate) fn family_packages(family: &str) -> Result<Vec<String>> {
    let family = to_wide(family);
    let mut count = 0u32;
    let mut length = 0u32;
    // SAFETY: null name/buffer pointers request required sizes only.
    let status = unsafe {
        GetPackagesByPackageFamily(
            family.as_ptr(),
            &mut count,
            ptr::null_mut(),
            &mut length,
            ptr::null_mut(),
        )
    };
    if status == ERROR_SUCCESS {
        return Ok(Vec::new());
    }
    if status != ERROR_INSUFFICIENT_BUFFER {
        bail!("GetPackagesByPackageFamily size query failed: {status}");
    }
    let mut names = vec![ptr::null_mut(); usize::try_from(count)?];
    let mut buffer = vec![0u16; usize::try_from(length)?];
    // SAFETY: `names` has `count` slots and `buffer` has `length` UTF-16 units.
    let status = unsafe {
        GetPackagesByPackageFamily(
            family.as_ptr(),
            &mut count,
            names.as_mut_ptr(),
            &mut length,
            buffer.as_mut_ptr(),
        )
    };
    if status != ERROR_SUCCESS {
        bail!("GetPackagesByPackageFamily failed: {status}");
    }
    let mut packages = Vec::with_capacity(usize::try_from(count)?);
    for &name in names.iter().take(usize::try_from(count)?) {
        let offset = name
            .addr()
            .checked_sub(buffer.as_ptr().addr())
            .context("Windows package name lies before the output buffer")?;
        anyhow::ensure!(
            offset.is_multiple_of(size_of::<u16>()),
            "Windows package name is not UTF-16 aligned"
        );
        let tail = buffer
            .get(offset / size_of::<u16>()..)
            .context("Windows package name lies outside the output buffer")?;
        packages.push(from_wide(tail)?);
    }
    Ok(packages)
}
