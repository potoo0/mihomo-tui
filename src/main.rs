use clap::{CommandFactory, FromArgMatches, ValueHint};

use crate::config::get_config_path;

mod action;
mod api;
mod app;
mod cli;
mod components;
mod config;
mod error;
mod logging;
mod models;
mod palette;
mod panic;
mod tui;
mod utils;
mod widgets;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    panic::init()?;

    // Enhance the help message for the config argument
    let def = get_config_path();
    let help = format!("Path to config file (default: {})", def.display());
    let cmd = cli::Args::command()
        .mut_arg("config", |a| a.help(help).value_hint(ValueHint::FilePath).next_line_help(true));
    let args = cli::Args::from_arg_matches(&cmd.get_matches())?;

    let config = config::Config::new(args.config)?;
    logging::init(&config)?;

    let api = api::Api::new(&config)?;
    if let Err(e) = api.get_version().await {
        tracing::error!("Failed to get version from API: {:?}", e);
        anyhow::bail!("`mihomo-api` unavailable, exiting: {:?}", e);
    }
    let mut app = app::App::new(config, api)?;
    app.run().await?;

    Ok(())
}
