use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::prelude::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph, Row, Table, TableState};
use strum::VariantArray;
use throbber_widgets_tui::{BRAILLE_SIX, Throbber, ThrobberState, WhichUse};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;
use tui_input::Input;
use unicode_segmentation::UnicodeSegmentation;

use crate::action::Action;
use crate::api::Api;
use crate::components::{Component, ComponentId, HORIZ_STEP};
use crate::models::dns::{DnsAnswer, DnsQueryRequest, DnsQueryResponse, DnsRecordType};
use crate::utils::input::KeyOutcome;
use crate::utils::text_ui::{popup_area, top_title_line};
use crate::utils::tui_input::input_request;
use crate::widgets::scrollable_navigator::ScrollableNavigator;
use crate::widgets::shortcut::{Fragment, Shortcut};

const FORM_HEIGHT: u16 = 3;
const STATUS_HEIGHT: u16 = 1;

type QueryResult = std::result::Result<DnsQueryResponse, String>;

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
enum FocusedField {
    #[default]
    Type,
    Name,
    Answers,
}

impl FocusedField {
    fn next(self) -> Self {
        match self {
            Self::Type => Self::Name,
            Self::Name => Self::Answers,
            Self::Answers => Self::Type,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Type => Self::Answers,
            Self::Name => Self::Type,
            Self::Answers => Self::Name,
        }
    }
}

#[derive(Default)]
pub struct DnsQueryComponent {
    api: Option<Arc<Api>>,
    action_tx: Option<UnboundedSender<Action>>,

    show: bool,
    focused: FocusedField,
    input: Input,
    record_type_index: usize,

    error: Option<String>,
    answers: Vec<DnsAnswer>,
    result_rx: Option<oneshot::Receiver<QueryResult>>,
    navigator: ScrollableNavigator,
    table_state: TableState,
    answer_horiz_offset: usize,

    loading: Arc<AtomicBool>,
    throbber: ThrobberState,
}

impl DnsQueryComponent {
    pub fn show(&mut self) {
        self.show = true;
        self.set_focused(FocusedField::Type);
    }

    pub fn hide(&mut self) {
        self.show = false;
        self.result_rx = None;
        self.loading.store(false, Ordering::Relaxed);

        self.reset_answers();
        self.answers.shrink_to_fit();
    }

    fn reset_table_state(&mut self) {
        self.navigator = Default::default();
        self.table_state = Default::default();
        self.answer_horiz_offset = 0;
    }

    fn reset_answers(&mut self) {
        self.answers.clear();
        self.error = None;
        self.reset_table_state();
    }

    fn finish_query(&mut self) {
        self.loading.store(false, Ordering::Relaxed);
        self.result_rx = None;
    }

    fn current_record_type(&self) -> DnsRecordType {
        DnsRecordType::VARIANTS.get(self.record_type_index).copied().unwrap_or(DnsRecordType::A)
    }

    fn next_record_type(&mut self) {
        let len = DnsRecordType::VARIANTS.len();
        if len > 0 {
            self.record_type_index = (self.record_type_index + 1) % len;
        }
    }

    fn prev_record_type(&mut self) {
        let len = DnsRecordType::VARIANTS.len();
        if len > 0 {
            self.record_type_index = self.record_type_index.checked_sub(1).unwrap_or(len - 1);
        }
    }

    fn set_focused(&mut self, focused: FocusedField) {
        if self.focused == focused {
            return;
        }

        self.focused = focused;
        if let Some(tx) = &self.action_tx {
            let _ = tx.send(Action::Shortcuts(self.shortcuts()));
        }
    }

    fn scrolled_answer_data<'a>(&self, data: &'a str) -> Cow<'a, str> {
        if self.answer_horiz_offset == 0 {
            Cow::Borrowed(data)
        } else {
            data.graphemes(true).skip(self.answer_horiz_offset).collect()
        }
    }

    fn query(&mut self) {
        if self.loading.load(Ordering::Relaxed) {
            return;
        }

        let name = self.input.value().trim();
        if name.is_empty() {
            self.error = Some("Name is required".into());
            return;
        }

        let Some(api) = self.api.as_ref().map(Arc::clone) else {
            self.error = Some("API is not initialized".into());
            return;
        };

        let req = DnsQueryRequest { name: name.to_owned(), r#type: self.current_record_type() };
        let (tx, rx) = oneshot::channel();
        self.result_rx = Some(rx);
        self.reset_answers();
        self.loading.store(true, Ordering::Relaxed);

        tokio::task::Builder::new()
            .name("dns-query")
            .spawn(async move {
                let result = api.query_dns(&req).await.map_err(|err| err.to_string());
                let _ = tx.send(result);
            })
            .unwrap();
    }

    fn poll_result(&mut self) {
        let Some(rx) = &mut self.result_rx else {
            return;
        };

        match rx.try_recv() {
            Ok(Ok(response)) => {
                self.answers = response.answer;
                self.error = None;
                self.reset_table_state();
                self.finish_query();
            }
            Ok(Err(err)) => {
                self.reset_answers();
                self.error = Some(err);
                self.finish_query();
            }
            Err(oneshot::error::TryRecvError::Empty) => {}
            Err(oneshot::error::TryRecvError::Closed) => {
                self.error = Some("DNS query task stopped".into());
                self.finish_query();
            }
        }
    }

    fn handle_focused_key_event(&mut self, key: KeyEvent) -> KeyOutcome {
        match self.focused {
            FocusedField::Type => match key.code {
                KeyCode::Left => self.prev_record_type(),
                KeyCode::Right => self.next_record_type(),
                _ => return KeyOutcome::Ignored,
            },
            FocusedField::Name => {
                let Some(req) = input_request(key) else {
                    return KeyOutcome::Ignored;
                };
                let _ = self.input.handle(req);
            }
            FocusedField::Answers => match key.code {
                KeyCode::Left => {
                    self.answer_horiz_offset = self.answer_horiz_offset.saturating_sub(HORIZ_STEP);
                }
                KeyCode::Right => {
                    self.answer_horiz_offset = self.answer_horiz_offset.saturating_add(HORIZ_STEP);
                }
                _ => return self.navigator.handle_key_event(false, key),
            },
        }

        KeyOutcome::Consumed
    }

    fn render_throbber(&mut self, frame: &mut Frame, area: Rect) {
        if !self.loading.load(Ordering::Relaxed) {
            return;
        }
        let symbol = Throbber::default()
            .label("Querying")
            .style(Style::default().fg(Color::White).bg(Color::Green).bold())
            .throbber_style(Style::default().fg(Color::White).bg(Color::Green).bold())
            .throbber_set(BRAILLE_SIX)
            .use_type(WhichUse::Spin);
        frame.render_stateful_widget(
            symbol,
            Rect::new(area.right().saturating_sub(11), area.y, 10, 1),
            &mut self.throbber,
        );
    }

    fn render_form(&self, frame: &mut Frame, area: Rect) {
        let [type_area, name_area] =
            Layout::horizontal([Constraint::Length(18), Constraint::Min(10)])
                .spacing(2)
                .areas(area);
        // render type
        let type_style = if self.focused == FocusedField::Type {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };
        let record_type = self.current_record_type();
        let type_line = Line::from(vec![
            Span::styled("< ", Style::default().fg(Color::DarkGray)),
            Span::styled(record_type.as_ref(), Style::default().fg(Color::LightCyan).bold()),
            Span::styled(" >", Style::default().fg(Color::DarkGray)),
        ]);
        let type_widget = Paragraph::new(type_line).block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(type_style)
                .title(" Type "),
        );
        frame.render_widget(type_widget, type_area);

        // render name input box
        let name_style = if self.focused == FocusedField::Name {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };
        let name_width = name_area.width.saturating_sub(2) as usize;
        let name_scroll = self.input.visual_scroll(name_width);
        let name = Paragraph::new(self.input.value()).scroll((0, name_scroll as u16)).block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(name_style)
                .title(" Name "),
        );
        frame.render_widget(name, name_area);
        if self.focused == FocusedField::Name {
            let x = self.input.visual_cursor().max(name_scroll) - name_scroll + 1;
            frame.set_cursor_position((name_area.x + x as u16, name_area.y + 1));
        }
    }

    fn render_status(&self, frame: &mut Frame, area: Rect) {
        if let Some(error) = &self.error {
            let line = Line::from(Span::styled(error, Style::default().fg(Color::Red)));
            frame.render_widget(Paragraph::new(line), area);
        }
    }

    fn render_answers(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.focused == FocusedField::Answers;
        let block_style = if focused { Style::default().fg(Color::Cyan) } else { Style::default() };
        let title = format!(" Answers ({}) ", self.answers.len());
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(block_style)
            .title(title);
        let viewport_len = area.height.saturating_sub(4) as usize;
        self.navigator.length(self.answers.len(), viewport_len);
        let records = self
            .answers
            .get(self.navigator.scroller.pos()..self.navigator.scroller.end_pos())
            .unwrap_or(&[]);
        *self.table_state.selected_mut() =
            self.navigator.focused.map(|v| v.saturating_sub(self.navigator.scroller.pos()));

        if self.answers.is_empty() && !self.loading.load(Ordering::Relaxed) {
            let message = if self.error.is_some() { "" } else { "No answer records" };
            frame.render_widget(Paragraph::new(message).block(block), area);
            return;
        }

        let header = Row::new(["NAME", "DATA"])
            .height(1)
            .bottom_margin(1)
            .style(Style::default().add_modifier(Modifier::BOLD));
        let rows = records.iter().map(|answer| {
            Row::new([Cow::Borrowed(answer.name.as_str()), self.scrolled_answer_data(&answer.data)])
        });
        let selected_row_style = Style::default().add_modifier(Modifier::REVERSED).fg(Color::Cyan);
        let table = Table::new(rows, [Constraint::Percentage(50), Constraint::Percentage(50)])
            .block(block)
            .header(header)
            .column_spacing(2)
            .row_highlight_style(selected_row_style);
        frame.render_stateful_widget(table, area, &mut self.table_state);
        self.navigator.render(frame, area);
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::vertical([
            Constraint::Length(FORM_HEIGHT),
            Constraint::Length(STATUS_HEIGHT),
            Constraint::Min(3),
        ])
        .split(area);
        self.render_form(frame, chunks[0]);
        self.render_status(frame, chunks[1]);
        self.render_answers(frame, chunks[2]);
    }
}

impl Component for DnsQueryComponent {
    fn id(&self) -> ComponentId {
        ComponentId::DnsQuery
    }

    fn shortcuts(&self) -> Vec<Shortcut> {
        let mut shortcuts = Vec::with_capacity(4);
        shortcuts.extend(vec![
            Shortcut::new(vec![Fragment::hl("⇧⇤"), Fragment::raw(" focus "), Fragment::hl("⇥")]),
            Shortcut::new(vec![Fragment::raw("query "), Fragment::hl("↵")]),
        ]);
        match self.focused {
            FocusedField::Type => shortcuts.push(Shortcut::new(vec![
                Fragment::hl("←"),
                Fragment::raw(" type "),
                Fragment::hl("→"),
            ])),
            FocusedField::Name => shortcuts.push(Shortcut::new(vec![
                Fragment::hl("←"),
                Fragment::raw(" cursor "),
                Fragment::hl("→"),
            ])),
            FocusedField::Answers => shortcuts.push(Shortcut::new(vec![
                Fragment::hl("←"),
                Fragment::raw("/"),
                Fragment::hl("↑"),
                Fragment::raw(" nav "),
                Fragment::hl("↓"),
                Fragment::raw("/"),
                Fragment::hl("→"),
            ])),
        }

        shortcuts
    }

    fn init(&mut self, api: Arc<Api>) -> Result<()> {
        self.api = Some(api);
        Ok(())
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if self.handle_focused_key_event(key).is_consumed() {
            return Ok(None);
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.hide();
                return Ok(Some(Action::Unfocus));
            }
            KeyCode::Tab => self.set_focused(self.focused.next()),
            KeyCode::BackTab => self.set_focused(self.focused.prev()),
            KeyCode::Enter => self.query(),
            _ => (),
        }

        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::DnsQuery => self.show(),
            Action::Focus(ComponentId::DnsQuery) => self.show(),
            Action::Tick => {
                self.poll_result();
                if self.loading.load(Ordering::Relaxed) {
                    self.throbber.calc_next();
                }
            }
            _ => (),
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        if !self.show {
            return Ok(());
        }

        let area = popup_area(area, 80, 80);
        frame.render_widget(Clear, area); // clears out the background
        // outer margin
        let area = area.inner(Margin::new(2, 1));

        let border = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Color::LightBlue)
            .title(top_title_line("dns query", Style::default()))
            .padding(Padding::symmetric(2, 1));
        let content_area = border.inner(area);
        frame.render_widget(border, area);
        self.render_throbber(frame, area);

        self.render(frame, content_area);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;
    use tokio::sync::mpsc::unbounded_channel;

    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn shortcuts_follow_focused_field() {
        let mut component = DnsQueryComponent::default();

        assert_eq!(
            component.shortcuts()[2],
            Shortcut::new(vec![Fragment::hl("←"), Fragment::raw(" type "), Fragment::hl("→")])
        );

        component.set_focused(FocusedField::Name);
        assert_eq!(
            component.shortcuts()[2],
            Shortcut::new(vec![Fragment::hl("←"), Fragment::raw(" cursor "), Fragment::hl("→")])
        );

        component.set_focused(FocusedField::Answers);
        assert_eq!(
            component.shortcuts()[2],
            Shortcut::new(vec![
                Fragment::hl("←"),
                Fragment::raw("/"),
                Fragment::hl("↑"),
                Fragment::raw(" nav "),
                Fragment::hl("↓"),
                Fragment::raw("/"),
                Fragment::hl("→"),
            ])
        );
    }

    #[test]
    fn focus_change_sends_shortcuts() {
        let mut component = DnsQueryComponent::default();
        let (tx, mut rx) = unbounded_channel();
        component.register_action_handler(tx).unwrap();

        component.handle_key_event(key(KeyCode::Tab)).unwrap();

        let action = rx.try_recv().unwrap();
        let Action::Shortcuts(shortcuts) = action else {
            panic!("expected shortcuts action");
        };
        assert_eq!(
            shortcuts[2],
            Shortcut::new(vec![Fragment::hl("←"), Fragment::raw(" cursor "), Fragment::hl("→")])
        );
    }

    #[test]
    fn answers_left_right_updates_horizontal_offset() {
        let mut component = DnsQueryComponent::default();
        component.set_focused(FocusedField::Answers);

        assert_eq!(component.answer_horiz_offset, 0);
        component.handle_key_event(key(KeyCode::Right)).unwrap();
        assert_eq!(component.answer_horiz_offset, HORIZ_STEP);
        component.handle_key_event(key(KeyCode::Left)).unwrap();
        assert_eq!(component.answer_horiz_offset, 0);
        component.handle_key_event(key(KeyCode::Left)).unwrap();
        assert_eq!(component.answer_horiz_offset, 0);
    }

    #[test]
    fn reset_answers_resets_horizontal_offset() {
        let mut component =
            DnsQueryComponent { answer_horiz_offset: HORIZ_STEP, ..Default::default() };

        component.reset_answers();

        assert_eq!(component.answer_horiz_offset, 0);
    }

    #[test]
    fn scrolled_answer_data_uses_graphemes() {
        let component = DnsQueryComponent { answer_horiz_offset: 1, ..Default::default() };

        assert_eq!(component.scrolled_answer_data("a中b"), "中b");
    }
}
