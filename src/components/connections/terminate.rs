use std::borrow::Cow;
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::prelude::{Color, Style};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph, Wrap};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use crate::api::Api;
use crate::app::action::Action;
use crate::components::{Component, ComponentContext, PopupComponent, PopupId};
use crate::models::Connection;
use crate::shortcut;
use crate::store::connections::CONNECTION_COLS;
use crate::ui::layout::{popup_area, top_title_line};
use crate::ui::render::RenderRequester;
use crate::ui::shortcut::{Fragment, Shortcut};
use crate::ui::symbols::key;
use crate::utils::columns::ColDef;

const COLS: [&ColDef<Connection>; 4] = [
    find_conn_def_by_id("host"),
    find_conn_def_by_id("rule"),
    find_conn_def_by_id("chains"),
    find_conn_def_by_id("source_ip"),
];

/// Popup field labels are padded to a fixed display width so values stay
/// aligned across languages.
const FIELD_LABEL_WIDTH: usize = 12;

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
    /// Returns the final display text; status variants are translated here.
    fn ui(&self) -> Option<(Color, Cow<'_, str>)> {
        match self {
            Phase::Terminating => Some((Color::Yellow, rust_i18n::t!("connections.terminating"))),
            Phase::DoneOk => Some((Color::Green, rust_i18n::t!("connections.terminated"))),
            Phase::DoneErr(e) => Some((Color::Red, e.into())),
            Phase::Hidden | Phase::Confirm => None,
        }
    }
}

#[derive(Debug)]
pub struct ConnectionTerminateComponent {
    api: Arc<Api>,
    render_requester: RenderRequester,

    token: CancellationToken,

    phase: Arc<RwLock<Phase>>,
    target: Option<Arc<Connection>>,
}

impl ConnectionTerminateComponent {
    pub fn new(ctx: &ComponentContext) -> Self {
        Self {
            api: Arc::clone(&ctx.api),
            render_requester: ctx.render.clone(),

            token: Default::default(),
            phase: Default::default(),
            target: Default::default(),
        }
    }

    pub fn show(&mut self, connection: Arc<Connection>) {
        self.token = CancellationToken::new();
        *self.phase.write().unwrap() = Phase::Confirm;
        self.target = Some(connection);
    }

    pub fn hide(&mut self) {
        self.token.cancel();
        *self.phase.write().unwrap() = Phase::Hidden;
        self.target = None;
    }

    fn terminate_connection(&mut self) -> Result<()> {
        debug!("Terminating connection: id={:?}", self.target.as_ref().map(|c| c.id.clone()));
        let phase = Arc::clone(&self.phase);
        *self.phase.write().unwrap() = Phase::Terminating;

        let api = self.api.clone();
        let id = self.target.as_deref().unwrap().id.clone();
        let token = self.token.clone();
        let render_requester = self.render_requester.clone();

        tokio::task::Builder::new().name("connection-terminator").spawn(async move {
            tokio::select! {
                _ = token.cancelled() => {
                    info!("Connection termination cancelled");
                }
                result = api.delete_connection(&id) => {
                    match result {
                        Ok(_) => *phase.write().unwrap() = Phase::DoneOk,
                        Err(e) => {
                            error!(error = ?e, "Failed to terminate connection");
                            *phase.write().unwrap() = Phase::DoneErr(e.to_string());
                        },
                    }
                    render_requester.request_render();
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

impl PopupComponent for ConnectionTerminateComponent {
    fn popup_id(&self) -> PopupId {
        PopupId::ConnectionTerminate
    }
}

impl Component for ConnectionTerminateComponent {
    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            shortcut!("yes", 0, Fragment::key(key::ENTER)),
            shortcut!("no", 0, Fragment::key("q"), Fragment::sep("/"), Fragment::key("Esc")),
        ]
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('n') | KeyCode::Esc
                if self.phase.read().unwrap().ne(&Phase::Terminating) =>
            {
                self.hide();
                return Ok(Some(self.close_action()));
            }
            KeyCode::Char('y') | KeyCode::Enter => {
                let should_term =
                    self.target.as_ref().is_some_and(|v| !v.inactive.load(Ordering::Relaxed)) && {
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
        let Some(conn) = self.target.as_deref() else {
            return Ok(());
        };

        // outer border
        let area = popup_area(area, 60, 50);
        frame.render_widget(Clear, area); // clears out the background
        let border = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Color::LightBlue)
            .title(top_title_line("titles.terminate", Style::default()))
            .padding(Padding::symmetric(2, 1));
        let inner = border.inner(area);
        frame.render_widget(border, area);
        let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).split(inner);

        // content
        let mut lines: Vec<Line> = COLS
            .iter()
            .map(|def| {
                let value = (def.accessor)(conn);
                Line::from(vec![
                    Span::styled(
                        padded_column_title(def.title),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(value),
                ])
            })
            .collect();
        lines.insert(0, Line::from(Span::raw(rust_i18n::t!("connections.confirm_terminate"))));
        lines.insert(1, Line::raw(""));
        let content = Paragraph::new(lines).wrap(Wrap { trim: true }).alignment(Alignment::Left);
        frame.render_widget(content, chunks[0]);

        // msg box
        if conn.inactive.load(Ordering::Relaxed) {
            render_msgbox(frame, chunks[1], Color::DarkGray, rust_i18n::t!("connections.inactive"));
            return Ok(());
        }

        if let Some((color, msg)) = phase.ui() {
            render_msgbox(frame, chunks[1], color, msg);
        }

        Ok(())
    }
}

const fn find_conn_def_by_id(id: &str) -> &ColDef<Connection> {
    let mut i = 0;

    while i < CONNECTION_COLS.len() {
        if CONNECTION_COLS[i].col.id.eq_ignore_ascii_case(id) {
            return &CONNECTION_COLS[i].col;
        }

        i += 1;
    }

    panic!("id not found")
}

fn render_msgbox(frame: &mut Frame, area: Rect, color: Color, msg: Cow<str>) {
    let block = Block::bordered().border_type(BorderType::Rounded).border_style(color);
    let paragraph = Paragraph::new(Span::styled(msg, Style::default().fg(color)))
        .block(block)
        .alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

/// Column titles are stable identifiers; translate them only for display.
fn padded_column_title(title: &str) -> String {
    let label = crate::i18n::column_title(title);
    let padding = FIELD_LABEL_WIDTH.saturating_sub(Span::raw(label.as_ref()).width());
    format!("{}{}", label, " ".repeat(padding))
}
