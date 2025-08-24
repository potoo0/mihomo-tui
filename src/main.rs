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
mod tui;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    errors::init()?;
    let args = cli::Args::parse();

    let config = config::Config::new(args.config)?;
    logging::init(config.clone())?;

    let mut app = app::App::new(config)?;
    app.run().await?;

    Ok(())
}
