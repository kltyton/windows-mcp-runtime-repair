use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use windows_registry::{CURRENT_USER, Key};

const REGISTRY_PATH: &str = r"Software\McpRuntimeRepair";
const VALUE_NOT_FOUND: i32 = -2_147_024_894;

#[derive(Clone, Debug)]
pub struct RepairPaths {
    pub root: PathBuf,
    pub temporary_root: PathBuf,
}

impl RepairPaths {
    pub fn current_user() -> Result<Self> {
        let local = env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is not set")?;
        let temporary_root = match read::<PathBuf>("TemporaryRoot")? {
            Some(path) => path,
            None => env::temp_dir(),
        };
        Ok(Self {
            root: PathBuf::from(local)
                .join("OpenAI")
                .join("Codex")
                .join("repair"),
            temporary_root,
        })
    }

    pub fn host(&self) -> PathBuf {
        self.root
            .join("hosts")
            .join(env!("CARGO_PKG_VERSION"))
            .join("runtime-host.exe")
    }

    pub fn plugins(&self) -> PathBuf {
        self.root.join("plugin-resources")
    }
}

fn registry() -> Result<Key> {
    CURRENT_USER
        .create(REGISTRY_PATH)
        .context("Open repair registration")
}

pub fn optional_string(key: &Key, name: &str) -> Result<Option<String>> {
    match key.get_string(name) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.code().0 == VALUE_NOT_FOUND => Ok(None),
        Err(error) => Err(error).context("Read registered setting"),
    }
}

pub fn read<T: DeserializeOwned>(name: &str) -> Result<Option<T>> {
    optional_string(&registry()?, name)?
        .map(|value| serde_json::from_str(&value).context("Read repair state"))
        .transpose()
}

pub fn write(name: &str, value: &impl Serialize) -> Result<()> {
    registry()?
        .set_string(name, serde_json::to_string(value)?)
        .context("Store repair state")
}

pub fn remove(name: &str) -> Result<()> {
    let key = registry()?;
    if optional_string(&key, name)?.is_some() {
        key.remove_value(name)
            .context("Remove owned repair registration")?;
    }
    Ok(())
}
