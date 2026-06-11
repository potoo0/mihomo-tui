use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, info, warn};

use super::{Component, ComponentId};
use crate::action::Action;
use crate::api::Api;
use crate::app_message::AppMessage;
use crate::config::Config;
use crate::utils::symbols::arrow;
use crate::utils::text_ui::{popup_area, top_title_line};
use crate::version_update::{SharedVersionUpdateState, VersionStatus, VersionUpdateState};
use crate::widgets::shortcut::{Fragment, Shortcut};

const CORE_UPGRADE_POLL_COUNT: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateTarget {
    /// Tui self
    App,
    /// mihomo core
    Core,
}

impl UpdateTarget {
    fn next(self) -> Self {
        match self {
            Self::App => Self::Core,
            Self::Core => Self::App,
        }
    }
}

pub struct UpdatesComponent {
    api: Option<Arc<Api>>,
    config: Option<Arc<Config>>,
    action_tx: Option<UnboundedSender<Action>>,
    update_state: SharedVersionUpdateState,
    selected: UpdateTarget,
    auto_restart: bool,
}

impl UpdatesComponent {
    pub fn new(update_state: SharedVersionUpdateState) -> Self {
        Self {
            api: None,
            config: None,
            action_tx: None,
            update_state,
            selected: UpdateTarget::App,
            auto_restart: true,
        }
    }

    fn show(&mut self) {
        self.selected = UpdateTarget::App;
        self.auto_restart = true;
    }

    fn snapshot(&self) -> VersionUpdateState {
        self.update_state.lock().clone()
    }

    fn toggle_auto_restart(&mut self) {
        self.auto_restart = !self.auto_restart;
    }

    fn refresh_versions(&self) -> Result<()> {
        let Some(api) = self.api.as_ref().map(Arc::clone) else {
            return Ok(());
        };
        let Some(mihomo_repo) = self.config.as_ref().map(|c| c.mihomo_repo.clone()) else {
            return Ok(());
        };
        debug!("refresh versions");
        let update_state = self.update_state.clone();
        tokio::task::Builder::new().name("app-version-refresher").spawn(async move {
            if let Err(e) = update_state.refresh(&api, &mihomo_repo).await {
                warn!(error = ?e, "Failed to refresh update status");
            }
        })?;
        Ok(())
    }

    fn update_core(&self, previous: VersionStatus) -> Result<()> {
        let Some(api) = self.api.as_ref().map(Arc::clone) else {
            return Ok(());
        };
        let Some(action_tx) = self.action_tx.as_ref().cloned() else {
            return Ok(());
        };
        let update_state = self.update_state.clone();
        let previous_version = match &previous {
            VersionStatus::Available { current, .. } => current.clone(),
            _ => return Ok(()),
        };

        {
            let mut state = update_state.lock();
            state.core = VersionStatus::Refreshing;
        }

        tokio::task::Builder::new().name("mihomo-core-upgrader").spawn(async move {
            match api.upgrade_core().await {
                Ok(()) => {
                    info!("Mihomo core upgrade requested successfully");
                    let _ = action_tx.send(Action::Info(
                        AppMessage::from((
                            "Update mihomo core",
                            "Mihomo core upgrade requested, waiting for service to restart...",
                        ))
                        .msg_box_size(45, 30),
                    ));
                    let mut upgraded_version = None;
                    for _ in 0..CORE_UPGRADE_POLL_COUNT {
                        tokio::time::sleep(Duration::from_secs(1)).await;

                        if let Some(version) =
                            api.get_version().await.ok().filter(|v| v.version != previous_version)
                        {
                            upgraded_version = Some(version);
                            break;
                        }
                    }
                    if let Some(upgraded_version) = upgraded_version {
                        info!(
                            upgraded_version = upgraded_version.version,
                            "Mihomo core upgrade successfully"
                        );
                        update_state.lock().core =
                            VersionStatus::UpToDate { current: upgraded_version.version.clone() };
                        let _ = action_tx.send(Action::CoreVersionUpdated(upgraded_version));
                    } else {
                        warn!(previous_version, "Timed out waiting for mihomo core upgrade");
                        update_state.lock().core = previous;
                        let _ = action_tx.send(Action::Error(
                            (
                                "Update mihomo core",
                                "Timed out waiting for mihomo core to restart with a new version",
                            )
                                .into(),
                        ));
                    }
                }
                Err(e) => {
                    warn!(error = ?e, "Failed to upgrade mihomo core");
                    update_state.lock().core = previous;
                    let _ = action_tx.send(Action::Error(("Update mihomo core", e).into()));
                }
            }
        })?;

        Ok(())
    }

    fn trigger_selected(&mut self) -> Result<Option<Action>> {
        let selected_status = {
            let guard = self.update_state.lock();
            match self.selected {
                UpdateTarget::App => match &guard.app {
                    VersionStatus::Available { .. } => guard.app.clone(),
                    _ => return Ok(None),
                },
                UpdateTarget::Core => match &guard.core {
                    VersionStatus::Available { .. } => guard.core.clone(),
                    _ => return Ok(None),
                },
            }
        };

        match self.selected {
            UpdateTarget::App => Ok(Some(Action::SelfUpdate(self.auto_restart))),
            UpdateTarget::Core => {
                self.update_core(selected_status)?;
                Ok(None)
            }
        }
    }

    fn item_line<'a>(
        &self,
        target: UpdateTarget,
        label: &'static str,
        status: &'a VersionStatus,
    ) -> Line<'a> {
        let selected = self.selected == target;
        let fg = if status.is_available() { Color::White } else { Color::DarkGray };
        let selector = if selected { arrow::RIGHT } else { " " };
        let style = if selected {
            Style::default().fg(fg).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(fg)
        };
        Line::from(vec![
            Span::styled(format!("{selector} "), style),
            Span::styled(label, style),
            Span::raw("  "),
            Span::styled(status.summary(), status_style(status)),
        ])
    }
}

impl Component for UpdatesComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Updates
    }

    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new(vec![Fragment::hl("⇧⇤"), Fragment::raw(" nav "), Fragment::hl("⇥")]),
            Shortcut::new(vec![Fragment::raw("toggle "), Fragment::hl("Space")]),
            Shortcut::new(vec![Fragment::raw("update "), Fragment::hl("↵")]),
            Shortcut::from("refresh", 0).unwrap(),
        ]
    }

    fn init(&mut self, api: Arc<Api>) -> Result<()> {
        self.api = Some(api);
        Ok(())
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn register_config_handler(&mut self, config: Arc<Config>) -> Result<()> {
        self.config = Some(config);
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(Some(Action::Unfocus)),
            KeyCode::Tab | KeyCode::BackTab => self.selected = self.selected.next(),
            KeyCode::Char(' ') => self.toggle_auto_restart(),
            KeyCode::Char('r') => self.refresh_versions()?,
            KeyCode::Enter => return self.trigger_selected(),
            _ => (),
        }

        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::AppUpdateRequest => self.show(),
            Action::RefreshVersion => self.refresh_versions()?,
            _ => (),
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let area = popup_area(area, 70, 50);
        frame.render_widget(Clear, area);

        let area = area.inner(Margin::new(2, 1));
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Color::LightBlue)
            .title(top_title_line("updates", Style::default()))
            .padding(Padding::symmetric(2, 1));
        let content_area = block.inner(area);
        frame.render_widget(block, area);

        let state = self.snapshot();
        let restart = if self.auto_restart { "yes" } else { "no" };
        let lines = vec![
            Line::from(vec![
                Span::raw("TUI auto restart? "),
                Span::styled(restart, Color::LightCyan),
            ]),
            Line::raw(""),
            self.item_line(UpdateTarget::App, "mihomo-tui ", &state.app),
            self.item_line(UpdateTarget::Core, "mihomo core", &state.core),
        ];
        frame.render_widget(Paragraph::new(lines), content_area);

        Ok(())
    }
}

fn status_style(status: &VersionStatus) -> Style {
    match status {
        VersionStatus::Available { .. } => Style::default().fg(Color::LightYellow),
        VersionStatus::Refreshing => Style::default().fg(Color::LightCyan),
        VersionStatus::Unknown => Style::default().fg(Color::DarkGray),
        VersionStatus::UpToDate { .. } => Style::default().fg(Color::Green),
    }
}
