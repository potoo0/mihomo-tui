use std::{env, thread};

use anyhow::{Context, Result, anyhow};
use tracing::{error, info};

use super::App;
use crate::app::action::Action;
use crate::app::message::AppMessage;
use crate::ui::terminal::Tui;
use crate::version_update::{self, RestartOutcome};

impl App {
    pub(super) fn handle_self_update(&mut self, tui: &mut Tui, restart: bool) -> Result<()> {
        let exe_path = env::current_exe().context("get current exe path")?;
        tui.exit()?;

        let action = match thread::spawn(version_update::update_app)
            .join()
            .map_err(|_| anyhow!("TUI upgrade thread panicked"))?
        {
            Ok(self_update::VersionStatus::UpToDate(version)) => Action::Info(
                AppMessage::from((
                    "actions.update_app",
                    rust_i18n::t!("updates.app_up_to_date", version = version).to_string(),
                ))
                .msg_box_size(45, 30),
            ),
            Ok(self_update::VersionStatus::Updated(version)) if restart => {
                self.runtime_config_saver.flush()?;
                info!(version, "TUI upgraded, trying to restart...");
                println!("TUI upgraded to {version}. Trying to restart...");
                match version_update::restart_app(&exe_path)? {
                    RestartOutcome::Restarted => return Ok(()),
                    RestartOutcome::Unsupported => Action::Info(
                        AppMessage::from((
                            "actions.update_app",
                            rust_i18n::t!("updates.app_updated_no_auto_restart", version = version)
                                .to_string(),
                        ))
                        .msg_box_size(45, 30),
                    ),
                }
            }
            Ok(self_update::VersionStatus::Updated(version)) => {
                println!("TUI upgraded to {version}. Please restart to use the new version.");
                Action::Info(
                    AppMessage::from((
                        "actions.update_app",
                        rust_i18n::t!("updates.app_updated_restart_required", version = version)
                            .to_string(),
                    ))
                    .msg_box_size(45, 30),
                )
            }
            Ok(status) => {
                return Err(anyhow!("unexpected self-update status: {status}"));
            }
            Err(e) => {
                error!(error = ?e, "TUI self-upgrade failed");
                Action::Error(("actions.update_app", e).into())
            }
        };

        tui.enter()?;
        tui.clear_screen()?;
        self.action_tx.send(action)?;
        self.action_tx.send(Action::RefreshVersion)?;

        Ok(())
    }
}
