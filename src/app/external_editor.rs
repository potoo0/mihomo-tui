use std::path::PathBuf;
use std::process::Command;

use anyhow::Result;
use tracing::{error, info};

use super::App;
use crate::app::action::Action;
use crate::ui::terminal::Tui;

impl App {
    pub(super) fn handle_spawn_external_editor(
        &self,
        tui: &mut Tui,
        editor: &str,
        filepath: &PathBuf,
    ) -> Result<()> {
        tui.exit()?;

        info!("Spawning external editor `{}` for file `{:?}`...", editor, filepath);
        // print to stdout, so that user can see it in terminal
        println!("Spawning external editor `{}` for file `{:?}`...", editor, filepath);
        match Command::new(editor).arg(filepath).status() {
            Ok(status) => {
                if !status.success() {
                    error!(
                        editor = editor,
                        status_code = ?status.code(),
                        "Editor exited with non-zero status"
                    );
                    let msg = rust_i18n::t!("editor.exit_status", editor = editor, status = status)
                        .to_string();
                    self.action_tx
                        .send(Action::Error(("actions.spawn_external_editor", msg).into()))?;
                }
            }
            Err(e) => {
                error!("Failed to spawn editor `{}`: {}", editor, e);
                self.action_tx.send(Action::Error(("actions.spawn_external_editor", e).into()))?;
            }
        }

        tui.enter()?;
        tui.clear_screen()?;

        Ok(())
    }
}
