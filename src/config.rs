#![allow(dead_code)] // Remove this once you start using the code

use color_eyre::Result;
use directories::ProjectDirs;
use eyre::{eyre, WrapErr};
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
        let config_path = get_config_path();
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

pub fn get_config_path() -> PathBuf {
    let dir = get_project_dir().config_dir().to_owned();
    if !dir.is_dir() {
        fs::create_dir_all(&dir)
            .with_context(|| format!("Fail to create directory `{}`", dir.display()))
            .unwrap();
    }

    dir.join("config.yaml")
}

pub fn get_project_dir() -> ProjectDirs {
    ProjectDirs::from("io.github", "", env!("CARGO_PKG_NAME"))
        .ok_or(eyre!("Fail to get project directory"))
        .unwrap()
}

#[cfg(target_os = "linux")]
#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::TempDir;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_config_default() {
        let temp = setup_temp_xdg();

        let config = Config::new().unwrap();
        assert_eq!(config.mihomo_api, "http://127.0.0.1:9093");
        assert_eq!(config.mihomo_secret, None);
        assert_eq!(config.log_level, Some("error".to_owned()));

        temp.close().unwrap();
    }

    #[test]
    fn test_config_existing_file() {
        let temp = setup_temp_xdg();

        let cfg_path = get_config_path();
        let custom_config = r#"
mihomo-api: "http://localhost"
mihomo-secret: "secret"
log-file: /tmp/log.log
log-level: "info"
"#;
        fs::write(&cfg_path, custom_config).unwrap();

        let config = Config::new().unwrap();
        assert_eq!(config.mihomo_api, "http://localhost");
        assert_eq!(config.mihomo_secret, Some("secret".to_owned()));
        assert_eq!(config.log_file, Some("/tmp/log.log".to_owned()));
        assert_eq!(config.log_level, Some("info".to_owned()));

        temp.close().unwrap();
    }

    #[test]
    fn test_config_ser_error() {
        let temp = setup_temp_xdg();

        let cfg_path = get_config_path();
        let partial_config = r#"
mihomo-api: "http://localhost"
log-file: ["/tmp/log.log"]
"#;
        fs::write(&cfg_path, partial_config).unwrap();

        let result = Config::new();
        assert!(result.is_err(), "expected error, got {:?}", result);

        let err_msg = result.unwrap_err().to_string();
        println!(">>> {err_msg}");
        assert!(
            err_msg.contains("Fail to deserialize file"),
            "expected contains `Fail to deserialize file`, but got {}",
            err_msg
        );

        temp.close().unwrap();
    }

    fn setup_temp_xdg() -> TempDir {
        // todo 使用隔离的 env var
        let temp = TempDir::new().unwrap();
        unsafe {
            env::set_var("XDG_DATA_HOME", temp.path());
        }
        temp
    }
}
