use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::app_message::AppMessage;
use crate::components::ComponentId;
use crate::models::{Connection, Version};
use crate::widgets::shortcut::Shortcut;

#[derive(Debug, Clone)]
pub enum Action {
    Tick,
    Render,
    Resize(u16, u16),
    Quit,
    Focus(ComponentId),
    Unfocus,
    Info(AppMessage),
    Error(AppMessage),
    AppUpdateRequest,
    SelfUpdate(bool),
    RefreshVersion,
    CoreVersionUpdated(Version),
    /// Spawn an external editor to edit a file. args: `(editor command, file path)`
    SpawnExternalEditor(String, PathBuf),
    Help,
    TabSwitch(ComponentId),
    Shortcuts(Vec<Shortcut>),
    ConnectionDetail(Arc<Connection>),
    ConnectionsSetting(Vec<String>),
    ConnectionsSettingChanged,
    /// Sent when connection layout settings change without affecting the data view.
    ConnectionsLayoutChanged,
    /// Sent when the filter pattern is changed via user input.
    FilterChanged(Option<String>),
    /// Programmatically sets the filter placeholder for the current tab.
    FilterPlaceholder(Option<String>),
    /// Programmatically sets the filter pattern without re-triggering `FilterChanged`.
    FilterSet(Option<String>),
    ConnectionTerminateRequest(Arc<Connection>),
    ConnectionBatchTerminateRequest(Vec<String>),
    ProxyDetail(String),
    ProxySetting,
    ProxySettingChanged,
    ProxyProviderDetail(String),
    DnsQuery,
}

/// Cloneable application action sender.
///
/// Regular actions are forwarded unchanged. Repeated render actions are
/// coalesced until the app starts handling the pending render.
#[derive(Debug, Clone)]
pub struct ActionTx {
    inner: UnboundedSender<Action>,
    render_pending: Arc<AtomicBool>,
}

impl ActionTx {
    pub fn channel() -> (Self, UnboundedReceiver<Action>) {
        let (inner, receiver) = mpsc::unbounded_channel();
        (Self { inner, render_pending: Arc::new(AtomicBool::new(false)) }, receiver)
    }

    pub fn send(&self, action: Action) -> Result<(), SendError<Action>> {
        let is_render = matches!(&action, Action::Render);
        if is_render
            && self
                .render_pending
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return Ok(());
        }

        match self.inner.send(action) {
            Ok(()) => Ok(()),
            Err(error) => {
                if is_render {
                    self.render_pending.store(false, Ordering::Release);
                }
                Err(error)
            }
        }
    }

    /// Allow a subsequent render request to be queued.
    ///
    /// The app calls this before drawing so that a request arriving during the
    /// draw schedules a follow-up frame.
    pub fn complete_render(&self) {
        self.render_pending.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, ActionTx};

    #[test]
    fn coalesces_render_actions_until_pending_state_is_cleared() {
        let (tx, mut rx) = ActionTx::channel();

        for _ in 0..10_000 {
            tx.send(Action::Render).unwrap();
        }

        assert!(matches!(rx.try_recv(), Ok(Action::Render)));
        assert!(rx.try_recv().is_err());

        tx.complete_render();
        tx.send(Action::Render).unwrap();

        assert!(matches!(rx.try_recv(), Ok(Action::Render)));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn forwards_regular_actions_without_coalescing() {
        let (tx, mut rx) = ActionTx::channel();

        tx.send(Action::Tick).unwrap();
        tx.send(Action::Tick).unwrap();

        assert!(matches!(rx.try_recv(), Ok(Action::Tick)));
        assert!(matches!(rx.try_recv(), Ok(Action::Tick)));
        assert!(rx.try_recv().is_err());
    }
}
