use crate::config;
use crate::config::Config;
use color_eyre::Result;
use std::fs::OpenOptions;
use std::path::PathBuf;
use tracing_error::ErrorLayer;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

pub fn init(config: Config) -> Result<()> {
    let log_level = config.log_level.unwrap_or(tracing::Level::INFO.to_string());
    let log_file = match &config.log_file {
        Some(path) => PathBuf::from(path),
        None => config::get_project_dir()
            .data_dir()
            .to_owned()
            .join(format!("{}.log", env!("CARGO_PKG_NAME"))),
    };
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)?;

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
