mod schema;
#[cfg(test)]
mod tests;

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

pub fn load(path: Option<PathBuf>) -> anyhow::Result<Config> {
    // If config file path is provided, read from it directly
    if let Some(ref config_path) = path {
        info!("Using config file at `{}`", config_path.display());
        return read_from_file(config_path);
    }

    let default_config: Config = serde_yaml_ng::from_str(DEFAULT_CONFIG)?;
    let config_path: PathBuf = get_config_path();
    info!("Using default config file at `{}`", config_path.display());
    // If config file does not exist, create one with default content
    if !config_path.is_file() {
        fs::write(&config_path, DEFAULT_CONFIG)
            .with_context(|| format!("Fail to write file `{}`", config_path.display()))?;
        info!("Created default config file at `{}`", config_path.display());
        return Ok(default_config);
    }

    read_from_file(&config_path)
}

fn read_from_file(path: &PathBuf) -> anyhow::Result<Config> {
    if !path.is_file() {
        return Err(anyhow!("Config file `{}` does not exist", path.display()));
    }
    let result =
        fs::File::open(path).with_context(|| format!("Fail to open file `{}`", path.display()))?;
    let cfg: Config = serde_yaml_ng::from_reader(result)
        .with_context(|| format!("Fail to deserialize file `{}`", path.display()))?;
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
