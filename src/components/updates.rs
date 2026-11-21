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
use tracing::{info, warn};

use crate::api::Api;
use crate::app::action::Action;
use crate::app::message::AppMessage;
use crate::components::{Component, ComponentContext, PopupComponent, PopupId};
use crate::shortcut;
use crate::ui::layout::{popup_area, top_title_line};
use crate::ui::render::RenderRequester;
use crate::ui::shortcut::{Fragment, Shortcut};
use crate::ui::symbols::{arrow, key};
use crate::version_update::{SharedVersionUpdateState, VersionStatus, VersionUpdateState};

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
    api: Arc<Api>,
    action_tx: UnboundedSender<Action>,
    render_requester: RenderRequester,

    update_state: SharedVersionUpdateState,
    selected: UpdateTarget,
    auto_restart: bool,
}

impl UpdatesComponent {
    pub fn new(ctx: &ComponentContext, update_state: SharedVersionUpdateState) -> Self {
        Self {
            api: Arc::clone(&ctx.api),
            action_tx: ctx.actions.clone(),
            render_requester: ctx.render.clone(),

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

    fn update_core(&self, previous: VersionStatus) -> Result<()> {
        let api = Arc::clone(&self.api);
        let action_tx = self.action_tx.clone();
        let update_state = self.update_state.clone();
        let render_requester = self.render_requester.clone();
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
                            "actions.update_mihomo_core",
                            rust_i18n::t!("updates.core_restart_waiting").to_string(),
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
                        warn!(previous_version, "Mihomo core upgrade timed out");
                        update_state.lock().core = previous;
                        let _ = action_tx.send(Action::Error(
                            (
                                "actions.update_mihomo_core",
                                rust_i18n::t!("updates.core_restart_timeout").to_string(),
                            )
                                .into(),
                        ));
                    }
                }
                Err(e) => {
                    warn!(error = ?e, "Failed to upgrade mihomo core");
                    update_state.lock().core = previous;
                    let _ = action_tx.send(Action::Error(("actions.update_mihomo_core", e).into()));
                }
            }
            render_requester.request_render();
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
        let fg = if status.is_available() { Color::Reset } else { Color::DarkGray };
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

impl PopupComponent for UpdatesComponent {
    fn popup_id(&self) -> PopupId {
        PopupId::Updates
    }
}

impl Component for UpdatesComponent {
    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new(vec![Fragment::key("q"), Fragment::sep("/"), Fragment::key("Esc")]),
            Shortcut::new(vec![
                Fragment::key(key::SHIFT_TAB),
                Fragment::label("shortcuts.nav"),
                Fragment::key(key::TAB),
            ]),
            Shortcut::new(vec![Fragment::key("Space"), Fragment::label("shortcuts.toggle")]),
            Shortcut::new(vec![Fragment::key(key::ENTER), Fragment::label("shortcuts.upgrade")]),
            shortcut!("refresh", 0),
        ]
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                return Ok(Some(self.close_action()));
            }
            KeyCode::Tab | KeyCode::BackTab => self.selected = self.selected.next(),
            KeyCode::Char(' ') => self.toggle_auto_restart(),
            KeyCode::Char('r') => return Ok(Some(Action::RefreshVersion)),
            KeyCode::Enter => return self.trigger_selected(),
            _ => (),
        }

        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        if matches!(action, Action::AppUpdateRequest) {
            self.show();
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
            .title(top_title_line("titles.updates", Style::default()))
            .padding(Padding::symmetric(2, 1));
        let content_area = block.inner(area);
        frame.render_widget(block, area);

        let state = self.snapshot();
        let restart = if self.auto_restart { "common.yes" } else { "common.no" };
        let lines = vec![
            Line::from(vec![
                Span::raw(rust_i18n::t!("updates.auto_restart_prompt")),
                Span::raw(" "),
                Span::styled(rust_i18n::t!(restart), Color::LightCyan),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::FocusTarget;

    #[test]
    fn refresh_is_requested_as_an_action_without_starting_popup_owned_work() {
        let mut component = UpdatesComponent::new(
            &ComponentContext::test_ctx(),
            SharedVersionUpdateState::default(),
        );
        // No Tokio runtime: the popup must delegate refresh to the application.
        let action = component.handle_key_event(KeyEvent::from(KeyCode::Char('r'))).unwrap();
        assert!(matches!(action, Some(Action::RefreshVersion)));
        let action = component.handle_key_event(KeyEvent::from(KeyCode::Esc)).unwrap();
        assert!(matches!(action, Some(Action::Unfocus(FocusTarget::Popup(PopupId::Updates)))));
    }
}
