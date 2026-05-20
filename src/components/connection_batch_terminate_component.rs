use std::sync::{Arc, RwLock};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::prelude::{Color, Line, Span, Style};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::action::Action;
use crate::api::Api;
use crate::components::{Component, ComponentId};
use crate::utils::text_ui::{popup_area, top_title_line};
use crate::widgets::shortcut::{Fragment, Shortcut};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Hidden,
    Confirm,
    Terminating,
    Done {
        ok: usize,
        err: usize,
    },
}

impl Phase {
    fn ui(&self) -> Option<(Color, String)> {
        match self {
            Phase::Terminating => Some((Color::Yellow, "Connections terminating...".to_string())),
            Phase::Done { ok, err } => {
                let color = if *err == 0 { Color::Green } else { Color::Yellow };
                Some((color, format!("Terminated {ok} connections, {err} failed.")))
            }
            Phase::Hidden | Phase::Confirm => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct ConnectionBatchTerminateComponent {
    api: Option<Arc<Api>>,
    token: CancellationToken,

    phase: Arc<RwLock<Phase>>,
    targets: Vec<String>,
}

impl ConnectionBatchTerminateComponent {
    pub fn show(&mut self, ids: Vec<String>) {
        self.token = CancellationToken::new();
        *self.phase.write().unwrap() = Phase::Confirm;
        self.targets = ids;
    }

    pub fn hide(&mut self) {
        self.token.cancel();
        *self.phase.write().unwrap() = Phase::Hidden;
        self.targets.clear();
    }

    fn terminate_connections(&mut self) -> Result<()> {
        debug!(num_conns = self.targets.len(), "Terminating filtered connections");
        let phase = Arc::clone(&self.phase);
        *self.phase.write().unwrap() = Phase::Terminating;

        let api = Arc::clone(self.api.as_ref().unwrap());
        let ids = self.targets.clone();
        let token = self.token.clone();

        tokio::task::Builder::new().name("connections-batch-terminator").spawn(async move {
            let mut ok = 0;
            let mut err = 0;

            for id in ids {
                tokio::select! {
                    _ = token.cancelled() => {
                        info!("Connections batch termination cancelled");
                        return;
                    }
                    result = api.delete_connection(&id) => {
                        match result {
                            Ok(_) => ok += 1,
                            Err(e) => {
                                err += 1;
                                debug!(error = ?e, connection_id = %id, "Failed to terminate connection");
                            }
                        }
                    }
                }
            }

            *phase.write().unwrap() = Phase::Done { ok, err };
        })?;

        Ok(())
    }

    fn render_msgbox(frame: &mut Frame, area: Rect, color: Color, msg: &str) {
        let block = Block::bordered().border_type(BorderType::Rounded).border_style(color);
        let paragraph = Paragraph::new(msg)
            .style(Style::default().fg(color))
            .block(block)
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
    }
}

impl Drop for ConnectionBatchTerminateComponent {
    fn drop(&mut self) {
        self.token.cancel();
        info!("`ConnectionBatchTerminateComponent` dropped, background task cancelled");
    }
}

impl Component for ConnectionBatchTerminateComponent {
    fn id(&self) -> ComponentId {
        ComponentId::ConnectionBatchTerminate
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
            KeyCode::Char('q') | KeyCode::Char('n') | KeyCode::Esc
                if self.phase.read().unwrap().ne(&Phase::Terminating) =>
            {
                self.hide();
                return Ok(Some(Action::Unfocus));
            }
            KeyCode::Char('y') | KeyCode::Enter
                if *self.phase.read().unwrap() == Phase::Confirm =>
            {
                self.terminate_connections()?;
            }
            _ => {}
        }
        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Quit => self.token.cancel(),
            Action::ConnectionBatchTerminateRequest(ids) => self.show(ids),
            _ => (),
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let phase = self.phase.read().unwrap().clone();
        if let Phase::Hidden = phase {
            return Ok(());
        }

        let area = popup_area(area, 60, 50);
        frame.render_widget(Clear, area);
        let border = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Color::LightBlue)
            .title(top_title_line("terminate", Style::default()))
            .padding(Padding::symmetric(2, 1));
        let inner = border.inner(area);
        frame.render_widget(border, area);
        let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).split(inner);

        let content = Paragraph::new(Line::from(vec![
            Span::raw("Are you sure to terminate "),
            Span::styled(self.targets.len().to_string(), Style::default().fg(Color::Yellow).bold()),
            Span::raw(" filtered connections?"),
        ]))
        .alignment(Alignment::Center);
        frame.render_widget(content, chunks[0]);

        if let Some((color, msg)) = phase.ui() {
            Self::render_msgbox(frame, chunks[1], color, &msg);
        }

        Ok(())
    }
}
