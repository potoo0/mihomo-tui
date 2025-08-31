use clap::Parser;

mod action;
mod api;
mod app;
mod cli;
mod components;
mod config;
mod errors;
mod logging;
mod models;
mod palette;
mod tui;
mod utils;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    errors::init()?;
    let args = cli::Args::parse();

    let config = config::Config::new(args.config)?;
    logging::init(&config)?;

    let api = api::Api::new(&config)?;
    let mut app = app::App::new(config, api)?;
    app.run().await?;

    Ok(())
}
