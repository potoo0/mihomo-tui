use std::{env, thread};

use anyhow::{Context, anyhow};

use crate::version_update::RestartOutcome;

mod action;
mod api;
mod app;
mod app_message;
mod cli;
mod components;
mod config;
mod logging;
mod models;
mod palette;
mod panic;
mod store;
mod tui;
mod utils;
mod version_update;
mod widgets;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    panic::init()?;

    let args = cli::parse_args()?;
    if args.update {
        let exe_path = env::current_exe().context("get current exe path")?;
        match thread::spawn(version_update::update_app)
            .join()
            .map_err(|_| anyhow!("app self update thread panicked"))?
        {
            Ok(self_update::Status::UpToDate(version)) => {
                println!("app is already up to date ({version}).");
            }
            Ok(self_update::Status::Updated(version)) => {
                println!("app updated to {version}.");
                return match version_update::restart_app(&exe_path)? {
                    RestartOutcome::Restarted => Ok(()),
                    RestartOutcome::Unsupported => {
                        println!(
                            "Auto restart is not supported on Windows. \
                             Please restart to use the new version."
                        );
                        Ok(())
                    }
                };
            }
            Err(e) => {
                tracing::error!(error = ?e, "app self update failed");
                anyhow::bail!("failed to update app: {e}");
            }
        }
    }

    let config = config::load(args.config)?;
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
