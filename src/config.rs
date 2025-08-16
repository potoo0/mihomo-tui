#![allow(dead_code)] // Remove this once you start using the code

use anyhow::{anyhow, Context};
use color_eyre::Result;
use directories::ProjectDirs;
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::{env, fs, path::PathBuf};
use tracing::error;

const DEFAULT_CONFIG: &str = include_str!("../.config/config.yaml");

#[derive(Debug, PartialEq, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub mihomo_api: String,
    pub mihomo_secret: Option<String>,
    pub log_level: String,
}

impl Config {
    pub fn new() -> Result<Self> {
        let default_config: Config = serde_yml::from_str(DEFAULT_CONFIG)?;
        // let data_dir = get_data_dir();
        let config_dir = get_project_dir();

        let cfg = default_config;
        // let mut builder = config::Config::builder()
        //     .set_default("data_dir", data_dir.to_str().unwrap())?
        //     .set_default("config_dir", config_dir.to_str().unwrap())?;
        //
        // let config_files = [
        //     ("config.json5", config::FileFormat::Json5),
        //     ("config.json", config::FileFormat::Json),
        //     ("config.yaml", config::FileFormat::Yaml),
        //     ("config.toml", config::FileFormat::Toml),
        //     ("config.ini", config::FileFormat::Ini),
        // ];
        // let mut found_config = false;
        // for (file, format) in &config_files {
        //     let source = config::File::from(config_dir.join(file))
        //         .format(*format)
        //         .required(false);
        //     builder = builder.add_source(source);
        //     if config_dir.join(file).exists() {
        //         found_config = true
        //     }
        // }
        // if !found_config {
        //     error!("No configuration file found. Application may not behave as expected");
        // }
        //
        // let mut cfg: Self = builder.build()?.try_deserialize()?;
        //
        // for (mode, default_bindings) in default_config.keybindings.iter() {
        //     let user_bindings = cfg.keybindings.entry(*mode).or_default();
        //     for (key, cmd) in default_bindings.iter() {
        //         user_bindings
        //             .entry(key.clone())
        //             .or_insert_with(|| cmd.clone());
        //     }
        // }
        // for (mode, default_styles) in default_config.styles.iter() {
        //     let user_styles = cfg.styles.entry(*mode).or_default();
        //     for (style_key, style) in default_styles.iter() {
        //         user_styles.entry(style_key.clone()).or_insert(*style);
        //     }
        // }

        Ok(cfg)
    }
}

pub fn get_project_dir() -> &'static PathBuf {
    static INSTANCE: OnceLock<PathBuf> = OnceLock::new();
    INSTANCE.get_or_init(|| {
        let dir = ProjectDirs::from("io.github", "", env!("CARGO_PKG_NAME"))
            .ok_or(anyhow!("Fail to get project directory"))
            .unwrap()
            .data_local_dir()
            .to_owned();
        if !dir.is_dir() {
            fs::create_dir_all(&dir)
                .with_context(|| format!("Fail to create directory `{}`", dir.display()))
                .unwrap();
        }

        dir
    })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_parse_style_default() {
        let config = Config::new().unwrap();
        let expected = Config {
            mihomo_api: "".into(),
            mihomo_secret: None,
            log_level: "debug".into(),
        };
        assert_eq!(&config, &expected);
    }
}
