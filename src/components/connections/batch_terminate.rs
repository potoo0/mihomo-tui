use std::borrow::Cow;
use std::sync::{Arc, RwLock};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::prelude::{Color, Line, Span, Style};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::api::Api;
use crate::app::action::Action;
use crate::components::{Component, ComponentContext, PopupComponent, PopupId};
use crate::shortcut;
use crate::ui::layout::{popup_area, top_title_line};
use crate::ui::render::RenderRequester;
use crate::ui::shortcut::{Fragment, Shortcut};
use crate::ui::symbols::key;

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
    fn ui(&self) -> Option<(Color, Cow<'_, str>)> {
        match self {
            Phase::Terminating => {
                Some((Color::Yellow, rust_i18n::t!("connections.terminating_filtered")))
            }
            Phase::Done { ok, err } => {
                let color = if *err == 0 { Color::Green } else { Color::Yellow };
                Some((color, rust_i18n::t!("connections.terminated_summary", ok = ok, err = err)))
            }
            Phase::Hidden | Phase::Confirm => None,
        }
    }
}

#[derive(Debug)]
pub struct ConnectionBatchTerminateComponent {
    api: Arc<Api>,
    render_requester: RenderRequester,

    token: CancellationToken,

    phase: Arc<RwLock<Phase>>,
    targets: Vec<String>,
}

impl ConnectionBatchTerminateComponent {
    pub fn new(ctx: &ComponentContext) -> Self {
        Self {
            api: Arc::clone(&ctx.api),
            render_requester: ctx.render.clone(),

            token: Default::default(),
            phase: Default::default(),
            targets: Default::default(),
        }
    }

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

        let api = Arc::clone(&self.api);
        let ids = self.targets.clone();
        let token = self.token.clone();
        let render_requester = self.render_requester.clone();

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
            render_requester.request_render();
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

impl PopupComponent for ConnectionBatchTerminateComponent {
    fn popup_id(&self) -> PopupId {
        PopupId::ConnectionBatchTerminate
    }
}

impl Component for ConnectionBatchTerminateComponent {
    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            shortcut!("yes", 0, Fragment::key(key::ENTER)),
            shortcut!("no", 0, Fragment::key("Esc")),
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
            .title(top_title_line("titles.terminate", Style::default()))
            .padding(Padding::symmetric(2, 1));
        let inner = border.inner(area);
        frame.render_widget(border, area);
        let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).split(inner);

        let template = rust_i18n::t!("connections.confirm_filtered");
        let content = Paragraph::new(confirmation_line(&template, self.targets.len()))
            .alignment(Alignment::Center);
        frame.render_widget(content, chunks[0]);

        if let Some((color, msg)) = phase.ui() {
            Self::render_msgbox(frame, chunks[1], color, &msg);
        }

        Ok(())
    }
}

fn confirmation_line(template: &str, count: usize) -> Line<'_> {
    let (before, after) = template
        .split_once("%{count}")
        .expect("confirmation translation must contain a count placeholder");
    Line::from(vec![
        Span::raw(before),
        Span::styled(count.to_string(), Style::default().fg(Color::Yellow).bold()),
        Span::raw(after),
    ])
}

#[cfg(test)]
mod tests {
    use strum::IntoEnumIterator;

    use super::*;
    use crate::i18n::Language;

    #[test]
    fn localized_confirmation_keeps_the_count_styled() {
        for (locale, expected) in [
            (Language::English, "Are you sure to terminate 3 filtered connections?"),
            (Language::SimplifiedChinese, "确定终止 3 个筛选出的连接？"),
        ] {
            let template = rust_i18n::t!("connections.confirm_filtered", locale = locale.code());
            assert_eq!(template.matches("%{count}").count(), 1);

            let line = confirmation_line(&template, 3);
            assert_eq!(line.spans[1].content, "3");
            assert_eq!(line.spans[1].style, Style::default().fg(Color::Yellow).bold());
            assert_eq!(
                line.spans.iter().map(|span| span.content.as_ref()).collect::<String>(),
                expected
            );
        }
    }

    #[test]
    fn every_language_confirmation_keeps_count_placeholder() {
        for language in Language::iter() {
            let template = rust_i18n::t!("connections.confirm_filtered", locale = language.code());
            assert_eq!(template.matches("%{count}").count(), 1, "{}", language.code());
        }
    }
}
