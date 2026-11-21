use std::sync::Arc;

use anyhow::Result;
use futures_util::{StreamExt, TryStreamExt, future};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{Mutex as AsyncMutex, mpsc, watch};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::api::Api;
use crate::components::ComponentId;
use crate::models::{Connection, ConnectionStats};

/// Owns the shared connections stream and its fan-out channels.
pub(super) struct ConnectionStreamHub {
    token: Option<CancellationToken>,
    stats_tx: watch::Sender<Option<ConnectionStats>>,
    conns_tx: mpsc::Sender<Vec<Connection>>,
    conns_rx: Arc<AsyncMutex<mpsc::Receiver<Vec<Connection>>>>,
}

impl ConnectionStreamHub {
    pub(super) fn new() -> Self {
        let (stats_tx, _) = watch::channel(None);
        let (conns_tx, conns_rx) = mpsc::channel(2);

        Self {
            token: Default::default(),
            stats_tx,
            conns_tx,
            conns_rx: Arc::new(AsyncMutex::new(conns_rx)),
        }
    }

    pub(super) fn stats_rx(&self) -> watch::Receiver<Option<ConnectionStats>> {
        self.stats_tx.subscribe()
    }

    pub(super) fn conns_rx(&self) -> Arc<AsyncMutex<mpsc::Receiver<Vec<Connection>>>> {
        Arc::clone(&self.conns_rx)
    }

    pub(super) fn is_active(&self) -> bool {
        self.token.as_ref().is_some_and(|token| !token.is_cancelled())
    }

    pub(super) fn needs_stream(component_id: ComponentId) -> bool {
        matches!(component_id, ComponentId::Overview | ComponentId::Connections)
    }

    pub(super) fn start_if_needed(
        &mut self,
        api: &Arc<Api>,
        current_tab: ComponentId,
    ) -> Result<()> {
        if !Self::needs_stream(current_tab) || self.is_active() {
            return Ok(());
        }

        let stream = api.stream_connections()?;
        let token = CancellationToken::new();
        let task_token = token.clone();
        info!(?current_tab, "Loading connections");
        let stats_tx = self.stats_tx.clone();
        let conns_tx = self.conns_tx.clone();
        let conns_rx = Arc::clone(&self.conns_rx);

        tokio::task::Builder::new().name("connections_wrapper-loader").spawn(async move {
            stream
                .take_until(task_token.cancelled())
                .inspect_err(|e| warn!(error = ?e, "Failed to parse connections."))
                .filter_map(|res| future::ready(res.ok()))
                .for_each(|record| {
                    stats_tx.send_replace(Some((&record).into()));
                    if let Err(TrySendError::Full(v)) =
                        conns_tx.try_send(record.connections.unwrap_or_default())
                    {
                        // drop oldest
                        if let Ok(mut guard) = conns_rx.try_lock() {
                            let _ = guard.try_recv();
                        }
                        let _ = conns_tx.try_send(v);
                    }
                    future::ready(())
                })
                .await;
        })?;
        self.token = Some(token);
        Ok(())
    }

    pub(super) fn stop_if_unused(&mut self, current_tab: ComponentId, has_idle_consumer: bool) {
        if self.is_active() && !Self::needs_stream(current_tab) && !has_idle_consumer {
            self.stop();
        }
    }

    pub(super) fn stop(&mut self) {
        if let Some(token) = self.token.take() {
            info!("Stopping connection stream");
            token.cancel();
        }
    }
}

impl Drop for ConnectionStreamHub {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_streams_are_needed_by_overview_and_connections_tabs() {
        assert!(ConnectionStreamHub::needs_stream(ComponentId::Overview));
        assert!(ConnectionStreamHub::needs_stream(ComponentId::Connections));
        assert!(!ConnectionStreamHub::needs_stream(ComponentId::Logs));
        assert!(!ConnectionStreamHub::needs_stream(ComponentId::Rules));
    }

    #[test]
    fn connection_stream_stops_only_without_current_or_idle_consumers() {
        let mut hub = ConnectionStreamHub::new();
        hub.stop_if_unused(ComponentId::Logs, false);
        assert!(!hub.is_active());

        hub.token = Some(CancellationToken::new());
        hub.stop_if_unused(ComponentId::Connections, false);
        assert!(hub.is_active());

        hub.stop_if_unused(ComponentId::Logs, true);
        assert!(hub.is_active());

        hub.stop_if_unused(ComponentId::Logs, false);
        assert!(!hub.is_active());
    }
}
