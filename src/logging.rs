use std::fs::OpenOptions;
use std::path::PathBuf;

use color_eyre::Result;
use color_eyre::eyre::WrapErr;
use tracing_error::ErrorLayer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

use crate::config::Config;

pub fn init(config: &Config) -> Result<()> {
    let log_file = match &config.log_file {
        Some(path) => PathBuf::from(path),
        None => return Ok(()),
    };
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
        .with_context(|| format!("Fail to open file `{}`", &log_file.display()))?;
    let log_level = config
        .log_level
        .clone()
        .unwrap_or(tracing::Level::INFO.to_string());

    // If the `RUST_LOG` environment variable is set, use that as the default, otherwise use the
    // value of the `LOG_ENV` environment variable. If the `LOG_ENV` environment variable contains
    // errors, then this will return an error.
    let env_filter = EnvFilter::try_new(&log_level)?;

    let file_subscriber = fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_writer(log_file)
        .with_target(false)
        .with_ansi(false)
        .with_filter(env_filter);

    tracing_subscriber::registry()
        .with(file_subscriber)
        .with(ErrorLayer::default())
        .try_init()?;

    Ok(())
}
