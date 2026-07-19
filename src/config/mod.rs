mod deserialize;
pub mod runtime;
mod schema;
#[cfg(test)]
mod tests;
pub mod validate;

use std::ops::Deref;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::{env, fs};

use anyhow::{Context, anyhow};
use directories::ProjectDirs;
pub use schema::*;
use tracing::info;

static DEFAULT_CONFIG: &str = include_str!("../../.config/config.yaml");
pub static PROJECT_NAME: LazyLock<&'static str> = LazyLock::new(|| {
    let s = env!("CARGO_CRATE_NAME").replace('-', "_").to_ascii_uppercase();
    Box::leak(s.into_boxed_str())
});

#[derive(Debug)]
pub struct LoadedConfig {
    pub config: Config,
    pub config_path: PathBuf,
    pub runtime_path: PathBuf,
}

impl Deref for LoadedConfig {
    type Target = Config;

    fn deref(&self) -> &Self::Target {
        &self.config
    }
}

impl LoadedConfig {
    pub fn try_apply_runtime(&mut self) {
        runtime::try_load_and_apply(&mut self.config, &self.runtime_path);
    }
}

pub fn load(path: Option<PathBuf>) -> anyhow::Result<LoadedConfig> {
    let explicit_config = path.is_some();
    let config_path = path.unwrap_or_else(get_config_path);
    let runtime_path = runtime::runtime_path_for(&config_path);
    info!("Using config file at `{}`", config_path.display());
    info!("Using runtime config file at `{}`", runtime_path.display());

    let mut config = if config_path.is_file() {
        read_from_file(&config_path)?
    } else {
        if explicit_config {
            return Err(anyhow!("Config file `{}` does not exist", config_path.display()));
        }

        // If default config file does not exist, create one with default content.
        let default_config = default_config()?;
        fs::write(&config_path, DEFAULT_CONFIG)
            .with_context(|| format!("Fail to write file `{}`", config_path.display()))?;
        info!("Created default config file at `{}`", config_path.display());
        default_config
    };

    if let Some(parent) = config_path.parent() {
        config.mihomo_api.resolve_relative_to(parent);
    }

    Ok(LoadedConfig { config, config_path, runtime_path })
}

pub(crate) fn default_config() -> anyhow::Result<Config> {
    let default_config: Config = yaml_serde::from_str(DEFAULT_CONFIG)?;
    default_config.validate()?;
    Ok(default_config)
}

fn read_from_file(path: &PathBuf) -> anyhow::Result<Config> {
    if !path.is_file() {
        return Err(anyhow!("Config file `{}` does not exist", path.display()));
    }
    let result =
        fs::File::open(path).with_context(|| format!("Fail to open file `{}`", path.display()))?;
    let cfg: Config = yaml_serde::from_reader(result)
        .with_context(|| format!("Fail to deserialize file `{}`", path.display()))?;
    cfg.validate().with_context(|| format!("Invalid config file `{}`", path.display()))?;
    Ok(cfg)
}

#[cfg(not(test))]
pub fn get_config_path() -> PathBuf {
    let dir = get_project_dir().config_dir().to_owned();
    if !dir.is_dir() {
        fs::create_dir_all(&dir)
            .with_context(|| format!("Fail to create directory `{}`", dir.display()))
            .unwrap();
    }

    dir.join("config.yaml")
}

#[cfg(test)]
pub fn get_config_path() -> PathBuf {
    use std::cell::RefCell;
    thread_local! {
        static TEST_CONFIG_PATH: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    }
    TEST_CONFIG_PATH.with(|cell| {
        let mut opt = cell.borrow_mut();
        if opt.is_none() {
            *opt = Some(temp_config_path());
        }
        opt.as_ref().unwrap().clone()
    })
}

#[cfg(test)]
pub fn temp_config_path() -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut path = env::temp_dir();
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    path.push(format!(".test_{}_{}.yaml", env!("CARGO_PKG_NAME"), nanos));
    path
}

#[allow(dead_code)]
pub fn get_project_dir() -> ProjectDirs {
    ProjectDirs::from("io.github", "potoo0", env!("CARGO_PKG_NAME"))
        .ok_or(anyhow!("Fail to determine project directory"))
        .unwrap()
}
