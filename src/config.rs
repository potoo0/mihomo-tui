#![allow(dead_code)] // Remove this once you start using the code

use color_eyre::Result;
use directories::ProjectDirs;
use eyre::{WrapErr, eyre};
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};

const DEFAULT_CONFIG: &str = include_str!("../.config/config.yaml");

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub mihomo_api: String,
    pub mihomo_secret: Option<String>,
    pub log_file: Option<String>,
    pub log_level: Option<String>,
}

impl Config {
    pub fn new() -> Result<Self> {
        let default_config: Config = serde_yml::from_str(DEFAULT_CONFIG)?;
        let config_path: PathBuf = get_config_path();
        if !config_path.is_file() {
            fs::write(&config_path, DEFAULT_CONFIG)
                .with_context(|| format!("Fail to write file `{}`", config_path.display()))?;
            return Ok(default_config);
        }

        let result = fs::File::open(&config_path)
            .with_context(|| format!("Fail to open file `{}`", config_path.display()))?;
        let cfg: Config = serde_yml::from_reader(result)
            .with_context(|| format!("Fail to deserialize file `{}`", config_path.display()))?;

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
    use std::{
        cell::RefCell,
        env,
        time::{SystemTime, UNIX_EPOCH},
    };
    thread_local! {
        static TEST_CONFIG_PATH: RefCell<Option<PathBuf>> = RefCell::new(None);
    }
    TEST_CONFIG_PATH.with(|cell| {
        let mut opt = cell.borrow_mut();
        if opt.is_none() {
            let mut path = env::temp_dir();
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            path.push(format!(".test_{}_{}.yaml", env!("CARGO_PKG_NAME"), nanos));
            *opt = Some(path);
        }
        opt.as_ref().unwrap().clone()
    })
}

pub fn get_project_dir() -> ProjectDirs {
    ProjectDirs::from("io.github", "", env!("CARGO_PKG_NAME"))
        .ok_or(eyre!("Fail to get project directory"))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_config_default() {
        let cfg_path = TempFile::new(get_config_path());
        let default_config: Config = serde_yml::from_str(DEFAULT_CONFIG).unwrap();

        let config = Config::new().unwrap();
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

        let config = Config::new().unwrap();
        assert_eq!(config.mihomo_api, "http://localhost");
        assert_eq!(config.mihomo_secret, Some("secret".to_owned()));
        assert_eq!(config.log_file, Some("/tmp/log.log".to_owned()));
        assert_eq!(config.log_level, Some("info".to_owned()));

        drop(cfg_path);
    }

    #[test]
    fn test_config_ser_error() {
        let cfg_path = TempFile::new(get_config_path());

        let partial_config = r#"
mihomo-api: "http://localhost"
log-file: ["/tmp/log.log"]
"#;
        fs::write(&cfg_path.0, partial_config).unwrap();

        let result = Config::new();
        assert!(result.is_err(), "expected error, got {:?}", result);

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Fail to deserialize file"),
            "expected contains `Fail to deserialize file`, but got {}",
            err_msg
        );

        drop(cfg_path);
    }

    fn remove_file(path: PathBuf) {
        if path.is_file() {
            let _ = fs::remove_file(path);
        }
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
