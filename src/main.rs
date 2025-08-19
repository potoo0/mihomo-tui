use clap::Parser;
use cli::Cli;
use color_eyre::Result;

use crate::app::App;

mod action;
mod app;
mod cli;
mod components;
mod config;
mod errors;
mod logging;
mod tui;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::Config::new()?;

    logging::init(config.clone())?;
    errors::init()?;

    let _ = Cli::parse();
    let mut app = App::new(config)?;
    app.run().await?;
    Ok(())
}
