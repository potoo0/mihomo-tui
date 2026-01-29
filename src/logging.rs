use std::fs::OpenOptions;
use std::path::PathBuf;

use anyhow::{Context, Result};
use tracing::Level;
use tracing_error::ErrorLayer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

use crate::config::{Config, PROJECT_NAME};

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

    // Resolve log filtering rules with the following priority:
    // 1. <PROJECT_NAME>_LOG_LEVEL (project-specific override)
    // 2. RUST_LOG (standard tracing environment variable)
    // 3. config.log_level (fallback, defaults to "info")
    let log_level = config.log_level.as_deref().unwrap_or(Level::INFO.as_str());
    let env_filter = EnvFilter::try_from_env(format!("{}_LOG_LEVEL", *PROJECT_NAME))
        .or_else(|_| EnvFilter::try_from_default_env())
        .or_else(|_| EnvFilter::try_new(log_level))?;

    let file_subscriber = fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_writer(log_file)
        .with_target(false)
        .with_ansi(false)
        .with_filter(env_filter);

    let registry = tracing_subscriber::registry().with(file_subscriber).with(ErrorLayer::default());

    #[cfg(debug_assertions)]
    let registry = registry.with(console_subscriber::spawn());

    registry.try_init()?;

    Ok(())
}
