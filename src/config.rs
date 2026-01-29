use std::path::PathBuf;
use std::sync::LazyLock;
use std::{env, fs};

use anyhow::{Context, Result, anyhow};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tracing::info;
use url::Url;

static DEFAULT_CONFIG: &str = include_str!("../.config/config.yaml");
pub static PROJECT_NAME: LazyLock<&'static str> = LazyLock::new(|| {
    let s = env!("CARGO_CRATE_NAME").replace('-', "_").to_ascii_uppercase();
    Box::leak(s.into_boxed_str())
});

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub mihomo_api: Url,
    pub mihomo_secret: Option<String>,
    pub mihomo_config_schema: Option<String>,
    pub log_file: Option<String>,

    /// Log filtering directives compatible with `tracing_subscriber::EnvFilter`.
    /// This field accepts the same syntax as `RUST_LOG`, for example:
    ///
    /// - `"info"` — set the global log level
    /// - `"info,mihomo_tui=trace"` — global `info`, override `mihomo_tui` to `trace`
    /// - `"mihomo_tui::api=debug"` — enable logs only for a specific module
    pub log_level: Option<String>,
}

impl Config {
    pub fn new(path: Option<PathBuf>) -> Result<Self> {
        // If config file path is provided, read from it directly
        if let Some(ref config_path) = path {
            info!("Using config file at `{}`", config_path.display());
            return Self::read_from_file(config_path);
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

        Self::read_from_file(&config_path)
    }

    fn read_from_file(path: &PathBuf) -> Result<Self> {
        if !path.is_file() {
            return Err(anyhow!("Config file `{}` does not exist", path.display()));
        }
        let result = fs::File::open(path)
            .with_context(|| format!("Fail to open file `{}`", path.display()))?;
        let cfg: Config = serde_yaml_ng::from_reader(result)
            .with_context(|| format!("Fail to deserialize file `{}`", path.display()))?;
        Ok(cfg)
    }
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
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};
    thread_local! {
        static TEST_CONFIG_PATH: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    }
    TEST_CONFIG_PATH.with(|cell| {
        let mut opt = cell.borrow_mut();
        if opt.is_none() {
            let mut path = env::temp_dir();
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            path.push(format!(".test_{}_{}.yaml", env!("CARGO_PKG_NAME"), nanos));
            *opt = Some(path);
        }
        opt.as_ref().unwrap().clone()
    })
}

#[allow(dead_code)]
pub fn get_project_dir() -> ProjectDirs {
    ProjectDirs::from("io.github", "potoo0", env!("CARGO_PKG_NAME"))
        .ok_or(anyhow!("Fail to determine project directory"))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let cfg_path = TempFile::new(get_config_path());
        let default_config: Config = serde_yaml_ng::from_str(DEFAULT_CONFIG).unwrap();

        let config = Config::new(None).unwrap();
        assert_eq!(config.mihomo_api, default_config.mihomo_api);
        assert_eq!(config.mihomo_secret, default_config.mihomo_secret);
        assert_eq!(config.log_file, default_config.log_file);
        assert_eq!(config.log_level, default_config.log_level);

        drop(cfg_path);
    }

    #[test]
    fn test_config_existing_file() {
        let cfg_path = TempFile::new(get_config_path());

        let custom_config = r#"
mihomo-api: "http://localhost"
mihomo-secret: "secret"
log-file: /tmp/log.log
log-level: "info"
"#;
        fs::write(&cfg_path.0, custom_config).unwrap();

        let config = Config::new(None).unwrap();
        assert_eq!(config.mihomo_api, Url::parse("http://localhost").unwrap());
        assert_eq!(config.mihomo_secret, Some("secret".to_owned()));
        assert_eq!(config.log_file, Some("/tmp/log.log".to_owned()));
        assert_eq!(config.log_level, Some("info".to_owned()));

        drop(cfg_path);
    }

    #[test]
    fn test_config_ser_error() {
        let cfg_path = TempFile::new(get_config_path());

        let partial_config = r#"
mihomo-api: "localhost"
"#;
        fs::write(&cfg_path.0, partial_config).unwrap();

        let result = Config::new(None);
        assert!(result.is_err(), "expected error, got {:?}", result);

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Fail to deserialize file"),
            "expected contains `Fail to deserialize file`, but got {}",
            err_msg
        );

        drop(cfg_path);
    }

    struct TempFile(PathBuf);

    impl TempFile {
        fn new(path: PathBuf) -> Self {
            Self(path)
        }

        fn remove(&self) {
            if self.0.is_file() {
                let _ = fs::remove_file(&self.0);
            }
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            self.remove();
        }
    }
}
