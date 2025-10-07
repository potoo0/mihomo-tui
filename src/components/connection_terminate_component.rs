use std::sync::{Arc, OnceLock, RwLock};

use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::prelude::{Color, Style};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph, Wrap};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::action::Action;
use crate::api::Api;
use crate::components::connections::CONNECTION_COLS;
use crate::components::{Component, ComponentId};
use crate::models::Connection;
use crate::utils::columns::ColDef;
use crate::utils::text_ui::{popup_area, top_title_line};
use crate::widgets::shortcut::{Fragment, Shortcut};

const COLS: [&str; 4] = ["host", "rule", "chains", "source_ip"];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Hidden,
    Confirm,
    Terminating,
    DoneOk,
    DoneErr(String),
}

impl Phase {
    fn ui(&self) -> Option<(Color, &str)> {
        match self {
            Phase::Terminating => Some((Color::Yellow, "Connection terminating...")),
            Phase::DoneOk => Some((Color::Green, "Connection terminated successfully.")),
            Phase::DoneErr(e) => Some((Color::Red, e.as_str())),
            Phase::Hidden | Phase::Confirm => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct ConnectionTerminateComponent {
    api: Option<Arc<Api>>,
    token: CancellationToken,

    phase: Arc<RwLock<Phase>>,
    target: Option<Arc<Connection>>,
}

impl ConnectionTerminateComponent {
    pub fn show(&mut self, connection: Arc<Connection>) {
        self.token = CancellationToken::new();
        *self.phase.write().unwrap() = Phase::Confirm;
        self.target = Some(connection);
    }

    fn cols_def() -> &'static [&'static ColDef<Connection>] {
        static HOST_RULE_COLS: OnceLock<Vec<&'static ColDef<Connection>>> = OnceLock::new();
        HOST_RULE_COLS
            .get_or_init(|| {
                COLS.iter()
                    .map(|id| match CONNECTION_COLS.iter().find(|c| c.id == *id) {
                        Some(c) => c,
                        None => panic!("Column definition for `{}` not found", id),
                    })
                    .collect()
            })
            .as_slice()
    }

    fn terminate_connection(&mut self) -> Result<()> {
        let phase = Arc::clone(&self.phase);
        *self.phase.write().unwrap() = Phase::Terminating;

        let api = self.api.as_ref().unwrap().clone();
        let id = self.target.as_deref().unwrap().id.clone();
        let token = self.token.clone();

        tokio::task::Builder::new().name("memory-loader").spawn(async move {
            tokio::select! {
                _ = token.cancelled() => {
                    info!("Connection termination cancelled");
                }
                result = api.delete_connection(&id) => {
                    match result {
                        Ok(_) => *phase.write().unwrap() = Phase::DoneOk,
                        Err(e) => {
                            error!("Failed to terminate connection: {}", e);
                            *phase.write().unwrap() = Phase::DoneErr(e.to_string());
                        },
                    }
                }
            }
        })?;

        Ok(())
    }
}

impl Drop for ConnectionTerminateComponent {
    fn drop(&mut self) {
        self.token.cancel();
        info!("`ConnectionTerminateComponent` dropped, background task cancelled");
    }
}

impl Component for ConnectionTerminateComponent {
    fn id(&self) -> ComponentId {
        ComponentId::ConnectionTerminate
    }

    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new(vec![Fragment::hl("y"), Fragment::raw("es "), Fragment::hl("↵")]),
            Shortcut::new(vec![Fragment::hl("n"), Fragment::raw("o "), Fragment::hl("Esc")]),
        ]
    }

    fn init(&mut self, api: Arc<Api>) -> Result<()> {
        self.api = Some(api);
        self.token = CancellationToken::new();
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.token.cancel();
                return Ok(Some(Action::Quit));
            }
            KeyCode::Char('q') | KeyCode::Char('n') | KeyCode::Esc => {
                if self.phase.read().unwrap().ne(&Phase::Terminating) {
                    self.token.cancel();
                    return Ok(Some(Action::Unfocus));
                }
            }
            KeyCode::Char('y') | KeyCode::Enter => {
                let should_term = {
                    let phase = self.phase.read().unwrap();
                    !matches!(*phase, Phase::Terminating | Phase::DoneOk)
                };
                if should_term {
                    self.terminate_connection()?;
                }
            }
            _ => {}
        };
        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Quit => self.token.cancel(),
            Action::ConnectionTerminateRequest(connection) => self.show(connection),
            _ => (),
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let phase = self.phase.read().unwrap().clone();
        if let Phase::Hidden = phase {
            return Ok(());
        }

        // outer border
        let area = popup_area(area, 60, 50);
        frame.render_widget(Clear, area); // clears out the background
        let border = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Color::LightBlue)
            .title(top_title_line("terminate", Style::default()))
            .padding(Padding::symmetric(2, 1));
        let inner = border.inner(area);
        frame.render_widget(border, area);
        let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).split(inner);

        // content
        let conn = self.target.as_deref().unwrap();
        let mut lines: Vec<Line> = Self::cols_def()
            .iter()
            .map(|def| {
                let value = (def.accessor)(conn);
                Line::from(vec![
                    Span::styled(
                        format!("{:<12}", def.title),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(value),
                ])
            })
            .collect();
        lines.insert(0, Line::from(Span::raw("Are you sure to terminate this connection?")));
        lines.insert(1, Line::raw(""));
        let content = Paragraph::new(lines).wrap(Wrap { trim: true }).alignment(Alignment::Left);
        frame.render_widget(content, chunks[0]);

        // status
        if let Some((color, msg)) = phase.ui() {
            let block = Block::bordered().border_type(BorderType::Rounded).border_style(color);
            let paragraph = Paragraph::new(Span::styled(msg, Style::default().fg(color)))
                .block(block)
                .alignment(Alignment::Center);
            frame.render_widget(paragraph, chunks[1]);
        }

        Ok(())
    }
}
