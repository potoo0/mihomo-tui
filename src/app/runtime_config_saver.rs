use std::path::PathBuf;

use anyhow::Result;
use tokio::sync::mpsc::UnboundedSender;
use tracing::error;

use crate::app::action::Action;
use crate::app::message::AppMessage;
use crate::config::runtime::{self, RuntimePatch};
use crate::store::hub::StoreHub;

const SAVE_DELAY_TICKS: u8 = 4;

/// Coalesces runtime-setting changes and persists the latest store snapshots.
pub(super) struct RuntimeConfigSaver {
    enabled: bool,
    path: PathBuf,
    stores: StoreHub,
    actions: UnboundedSender<Action>,
    dirty: bool,
    remaining_ticks: Option<u8>,
}

impl RuntimeConfigSaver {
    pub fn new(
        enabled: bool,
        path: PathBuf,
        stores: StoreHub,
        actions: UnboundedSender<Action>,
    ) -> Self {
        Self { enabled, path, stores, actions, dirty: false, remaining_ticks: None }
    }

    pub fn on_action(&mut self, action: &Action) {
        if !self.enabled {
            return;
        }
        match action {
            Action::LanguageChanged(_)
            | Action::ConnectionsSettingChanged
            | Action::ProxySettingChanged
            | Action::RuntimeConfigChanged => {
                self.dirty = true;
                self.remaining_ticks = Some(SAVE_DELAY_TICKS);
            }
            Action::Tick => {
                if let Some(remaining) = &mut self.remaining_ticks {
                    *remaining = remaining.saturating_sub(1);
                }
            }
            _ => {}
        }
    }

    /// Call after draining the action batch, including queued changes on quit.
    pub fn finish_batch(&mut self, should_quit: bool) -> Result<()> {
        if should_quit || self.remaining_ticks == Some(0) {
            self.flush()?;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        if !self.enabled || !self.dirty {
            return Ok(());
        }
        // Keep dirty on failure, but wait for another change or exit to retry.
        self.remaining_ticks = None;
        match self.save() {
            Ok(()) => self.dirty = false,
            Err(e) => {
                error!(error = ?e, "Failed to save runtime config");
                self.actions.send(Action::Error(
                    AppMessage::from(("actions.save_runtime_config", e)).msg_box_size(60, 30),
                ))?;
            }
        }
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let patch = RuntimePatch {
            language: Some(crate::i18n::language()),
            ..RuntimePatch::try_from(&self.stores)?
        };
        runtime::save(&self.path, patch)
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc::{self, UnboundedReceiver};

    use super::*;
    use crate::store::proxy::{ProxyDetailSetting, ProxySetting};

    fn saver_with_runtime_path(path: PathBuf) -> (RuntimeConfigSaver, UnboundedReceiver<Action>) {
        let (actions, rx) = mpsc::unbounded_channel();
        (RuntimeConfigSaver::new(true, path, StoreHub::default(), actions), rx)
    }

    fn save_tick(saver: &mut RuntimeConfigSaver) {
        saver.on_action(&Action::Tick);
        saver.finish_batch(false).unwrap();
    }

    #[test]
    fn runtime_changes_share_one_countdown_and_save_the_latest_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.runtime.yaml");
        let (mut saver, _rx) = saver_with_runtime_path(path.clone());
        let setting = saver.stores.register(ProxyDetailSetting::default()).unwrap();

        // Every persistence-related action resets the same countdown.
        for action in [
            Action::LanguageChanged(crate::i18n::Language::English),
            Action::ConnectionsSettingChanged,
            Action::ProxySettingChanged,
            Action::RuntimeConfigChanged,
        ] {
            saver.on_action(&action);
            saver.finish_batch(false).unwrap();
            for _ in 0..3 {
                save_tick(&mut saver);
            }
            assert!(!path.exists());
        }

        setting.update(|setting| setting.card_width = Some(35));
        saver.on_action(&Action::TabSwitch(crate::components::ComponentId::Connections));
        save_tick(&mut saver);
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("card-width: 35"));
        assert!(!saver.dirty);

        std::fs::remove_file(&path).unwrap();
        for _ in 0..8 {
            save_tick(&mut saver);
        }
        saver.finish_batch(true).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn a_change_later_in_the_batch_defers_a_due_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.runtime.yaml");
        let (mut saver, _rx) = saver_with_runtime_path(path.clone());
        saver.on_action(&Action::RuntimeConfigChanged);
        for _ in 0..4 {
            saver.on_action(&Action::Tick);
        }
        saver.on_action(&Action::RuntimeConfigChanged);
        saver.finish_batch(false).unwrap();
        assert!(!path.exists());
        for _ in 0..4 {
            save_tick(&mut saver);
        }
        assert!(path.exists());
    }

    #[test]
    fn quit_flushes_changes_queued_after_the_quit_action() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.runtime.yaml");
        let (mut saver, mut rx) = saver_with_runtime_path(path.clone());
        let setting = saver.stores.register(ProxySetting::default()).unwrap();
        setting.update(|setting| setting.test_url = "https://latest.example/test".into());
        saver.actions.send(Action::Quit).unwrap();
        saver.actions.send(Action::ProxySettingChanged).unwrap();
        saver.actions.send(Action::RuntimeConfigChanged).unwrap();

        let mut should_quit = false;
        while let Ok(action) = rx.try_recv() {
            should_quit |= matches!(action, Action::Quit);
            saver.on_action(&action);
            assert!(!path.exists());
        }
        saver.finish_batch(should_quit).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("https://latest.example/test"));
        assert!(!saver.dirty);
    }

    #[test]
    fn failed_save_stays_dirty_without_repeated_errors_and_retries_on_quit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.runtime.yaml");
        std::fs::create_dir(&path).unwrap();
        let (mut saver, mut rx) = saver_with_runtime_path(path.clone());
        saver.on_action(&Action::RuntimeConfigChanged);
        for _ in 0..4 {
            save_tick(&mut saver);
        }
        assert!(saver.dirty);
        assert!(matches!(rx.try_recv(), Ok(Action::Error(_))));
        for _ in 0..8 {
            save_tick(&mut saver);
        }
        assert!(rx.try_recv().is_err());

        std::fs::remove_dir(&path).unwrap();
        saver.finish_batch(true).unwrap();
        assert!(path.is_file());
        assert!(!saver.dirty);
    }

    #[test]
    fn disabled_runtime_config_does_not_create_or_modify_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.runtime.yaml");
        let (actions, mut rx) = mpsc::unbounded_channel();
        let mut saver = RuntimeConfigSaver::new(false, path.clone(), StoreHub::default(), actions);

        saver.on_action(&Action::RuntimeConfigChanged);
        saver.on_action(&Action::ConnectionsSettingChanged);
        for _ in 0..4 {
            save_tick(&mut saver);
        }
        saver.finish_batch(true).unwrap();
        assert!(!saver.dirty);

        saver.flush().unwrap();
        assert!(!path.exists());

        let existing = "existing runtime settings";
        std::fs::write(&path, existing).unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        saver.on_action(&Action::RuntimeConfigChanged);
        saver.flush().unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), existing);
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), modified);

        // An invalid destination would fail if saving were attempted.
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        saver.on_action(&Action::RuntimeConfigChanged);
        saver.flush().unwrap();
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn runtime_config_preserves_existing_sidecar_when_stores_are_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.runtime.yaml");
        std::fs::write(
            &path,
            "$schema-version: 2\nproxy-setting:\n  test-url: https://existing.example/generate_204\n",
        )
        .unwrap();

        let (mut saver, mut rx) = saver_with_runtime_path(path.clone());
        saver.on_action(&Action::RuntimeConfigChanged);
        saver.flush().unwrap();
        assert!(rx.try_recv().is_err());

        let raw = std::fs::read_to_string(path).unwrap();
        assert!(raw.contains("test-url: https://existing.example/generate_204"));
    }
}
