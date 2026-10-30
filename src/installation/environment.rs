use anyhow::{Context, Result};
use windows_registry::CURRENT_USER;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};

use super::state::optional_string;

pub const CLI_SETTING: &str = "CODEX_CLI_PATH";
pub const PLUGIN_SETTING: &str = "CODEX_ELECTRON_BUNDLED_PLUGINS_RESOURCES_PATH";

pub fn read(name: &str) -> Result<Option<String>> {
    optional_string(&CURRENT_USER.create("Environment")?, name)
}

pub fn write(name: &str, value: Option<&str>) -> Result<()> {
    let key = CURRENT_USER.create("Environment")?;
    match value {
        Some(value) => key
            .set_string(name, value)
            .context("Set current-user backend configuration"),
        None => {
            if optional_string(&key, name)?.is_some() {
                key.remove_value(name)
                    .context("Remove owned backend configuration")?;
            }
            Ok(())
        }
    }
}

pub fn notify() {
    let text: Vec<u16> = "Environment".encode_utf16().chain(Some(0)).collect();
    let mut result = 0;
    // SAFETY: the immutable NUL-terminated string and writable result remain live
    // throughout this synchronous configuration notification. No process is stopped.
    let delivered = unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            text.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            2000,
            &mut result,
        )
    };
    if delivered == 0 {
        eprintln!(
            "Environment notification timed out; saved settings remain available to new logon sessions"
        );
    }
}
