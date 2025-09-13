use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use futures_util::{StreamExt, TryStreamExt, future};
use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::prelude::{Modifier, Stylize};
use ratatui::style::{Color, Style};
use ratatui::symbols::line;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, List, ListItem, ListState, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use strum::IntoEnumIterator;
use throbber_widgets_tui::{Throbber, ThrobberState};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::action::Action;
use crate::api::Api;
use crate::components::logs::Logs;
use crate::components::shortcut::{Fragment, Shortcut};
use crate::components::{Component, ComponentId};
use crate::models::LogLevel;
use crate::utils::symbols::arrow;
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT};

const ROW_HEIGHT: usize = 1;

#[derive(Default)]
pub struct LogsComponent {
    api: Option<Arc<Api>>,
    token: CancellationToken,
    store: Arc<Logs>,
    level: Option<LogLevel>,
    live_mode: Arc<AtomicBool>,
    filter_pattern: Arc<Mutex<Option<String>>>,

    level_changed: bool,
    filter_pattern_changed: bool,

    viewport: u16,
    item_size: usize,
    list_state: ListState,
    scroll_state: ScrollbarState,
    throbber_state: ThrobberState,
    action_tx: Option<UnboundedSender<Action>>,
}

impl LogsComponent {
    pub fn new() -> Self {
        let mut s = Self::default();
        s.live_mode = Arc::new(AtomicBool::new(true));
        s
    }

    fn load_log(&mut self) -> Result<()> {
        info!("Loading log, with level: {:?}", self.level);
        let token = self.token.clone();
        let api = Arc::clone(self.api.as_ref().unwrap());
        let store = Arc::clone(&self.store);
        let level = self.level;
        let filter_pattern = Arc::clone(&self.filter_pattern);
        let live_mode = Arc::clone(&self.live_mode);

        tokio::task::Builder::new().name("log-loader").spawn(async move {
            let stream = match api.get_logs(level).await {
                Ok(stream) => stream,
                Err(e) => {
                    warn!("Failed to get memory stream: {e}");
                    return;
                }
            };
            stream
                .take_until(token.cancelled())
                .inspect_err(|e| warn!("Failed to parse log: {e}"))
                .filter_map(|res| future::ready(res.ok()))
                .for_each(|record| {
                    store.push(record);
                    if live_mode.load(Ordering::Relaxed) {
                        let filter_pattern = filter_pattern.lock().unwrap();
                        let filter_pattern = filter_pattern.as_deref();
                        store.compute_view(filter_pattern);
                    }
                    future::ready(())
                })
                .await;
        })?;
        Ok(())
    }

    fn level_style(level: &LogLevel) -> Style {
        match level {
            LogLevel::Error => Style::default().fg(Color::Red),
            LogLevel::Warning => Style::default().fg(Color::Magenta),
            LogLevel::Info => Style::default().fg(Color::Yellow),
            LogLevel::Debug => Style::default().fg(Color::Blue),
        }
    }

    fn level_shortcuts<'a>(&mut self) -> Vec<Span<'a>> {
        let mut vec = Vec::with_capacity(8);
        vec.push(Span::raw(TOP_TITLE_LEFT));
        vec.push(Span::raw("level: "));
        for (idx, lv) in LogLevel::iter().enumerate() {
            if idx > 0 {
                vec.push(Span::raw("/"));
            }
            let label = lv.to_string();
            if let Some(cur) = &self.level
                && cur == &lv
            {
                vec.push(Span::styled(label, Self::level_style(&lv)));
            } else {
                vec.extend(Shortcut::from(label, 0).unwrap().into_spans(None));
            }
        }
        vec.push(Span::raw(TOP_TITLE_RIGHT));
        vec
    }

    fn render_list(&mut self, frame: &mut Frame, area: Rect) {
        let records = self.store.view();
        self.item_size = records.len();
        self.scroll_state = self.scroll_state.content_length(self.item_size * ROW_HEIGHT);
        self.viewport = area.height.saturating_sub(2); // borders

        // TODO: Implement virtualized rendering: only render rows within the visible viewport
        let items: Vec<ListItem> = records
            .iter()
            .rev()
            .map(|item| {
                let content = vec![
                    Span::styled(format!(" {:<9}", item.r#type), Self::level_style(&item.r#type)),
                    Span::raw(item.payload.as_str()),
                ];
                // LOG_COLS.iter().map(|def| (def.accessor)(item)).map(Span::from).collect();
                ListItem::new(Line::from(content))
            })
            .collect();
        let mut title_line = Line::from(vec![
            Span::raw(TOP_TITLE_LEFT),
            Span::raw("logs ("),
            Span::styled(
                self.list_state.selected().map(|i| (i + 1).to_string()).unwrap_or("-".into()),
                Color::LightCyan,
            ),
            Span::raw("/"),
            Span::styled(self.item_size.to_string(), Color::Cyan),
            Span::raw(")"),
            Span::raw(TOP_TITLE_RIGHT),
        ]);
        title_line.extend(self.level_shortcuts());
        let block = Block::bordered().border_type(BorderType::Rounded).title(title_line);
        let selected_style = Style::default().add_modifier(Modifier::REVERSED).fg(Color::Cyan);
        let logs = List::new(items).block(block).highlight_style(selected_style);
        frame.render_stateful_widget(logs, area, &mut self.list_state);

        let (throbber_label, throbber_color) = if self.live_mode.load(Ordering::Relaxed) {
            ("Live  ", Color::Green)
        } else {
            ("Paused", Color::Red)
        };
        let symbol = Throbber::default()
            .label(throbber_label)
            .style(Style::default().bg(throbber_color).bold())
            .throbber_style(Style::default().bg(throbber_color).bold())
            .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
            .use_type(throbber_widgets_tui::WhichUse::Spin);
        frame.render_stateful_widget(
            symbol,
            Rect::new(area.right().saturating_sub(9), area.y, 8, 1),
            &mut self.throbber_state,
        );
    }

    fn render_scrollbar(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .track_symbol(Some(line::VERTICAL))
                .begin_symbol(Some(arrow::UP))
                .end_symbol(Some(arrow::DOWN)),
            area.inner(Margin::new(1, 1)),
            &mut self.scroll_state,
        );
    }

    pub fn next_row(&mut self) {
        if self.item_size == 0 {
            return;
        }
        let i = self
            .list_state
            .selected()
            .map_or(0, |i| if i + 1 >= self.item_size { 0 } else { i + 1 });
        self.list_state.select(Some(i));
        self.scroll_state = self.scroll_state.position(i * ROW_HEIGHT);
    }

    pub fn prev_row(&mut self) {
        if self.item_size == 0 {
            return;
        }
        let i = self
            .list_state
            .selected()
            .map_or(0, |i| if i == 0 { self.item_size - 1 } else { i - 1 });
        self.list_state.select(Some(i));
        self.scroll_state = self.scroll_state.position(i * ROW_HEIGHT);
    }

    pub fn first_row(&mut self) {
        if self.item_size == 0 {
            return;
        }
        self.list_state.select(Some(0));
        self.scroll_state = self.scroll_state.position(0);
    }

    pub fn last_row(&mut self) {
        if self.item_size == 0 {
            return;
        }
        let i = self.item_size - 1;
        self.list_state.select(Some(i));
        self.scroll_state = self.scroll_state.position(i * ROW_HEIGHT);
    }

    fn live_mode(&mut self, live_mode: bool) {
        self.live_mode.store(live_mode, Ordering::Relaxed);
        if live_mode {
            self.list_state.select(None);
            self.scroll_state = self.scroll_state.position(0);
        }
    }

    fn set_level(&mut self, level: LogLevel) {
        if let Some(lv) = &self.level
            && lv == &level
        {
            return;
        }
        self.level = Some(level);
        self.level_changed = true;
    }
}

impl Drop for LogsComponent {
    fn drop(&mut self) {
        self.token.cancel();
        info!("`LogsComponent` dropped, background task cancelled");
    }
}

impl Component for LogsComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Logs
    }

    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new(vec![
                Fragment::hl(arrow::UP),
                Fragment::raw(" select "),
                Fragment::hl(arrow::DOWN),
            ]),
            Shortcut::new(vec![Fragment::raw("first "), Fragment::hl("g")]),
            Shortcut::new(vec![Fragment::raw("last "), Fragment::hl("G")]),
            Shortcut::new(vec![Fragment::raw("live "), Fragment::hl("Esc")]),
        ]
    }

    fn init(&mut self, api: Arc<Api>) -> Result<()> {
        self.api = Some(api);
        self.token = CancellationToken::new();
        self.load_log()?;

        Ok(())
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Esc => self.live_mode(true),
            KeyCode::Char('g') => {
                self.first_row();
                self.live_mode(false);
            }
            KeyCode::Char('G') => {
                self.last_row();
                self.live_mode(false);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.next_row();
                self.live_mode(false);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.prev_row();
                self.live_mode(false);
            }
            KeyCode::Char('f') => return Ok(Some(Action::Focus(ComponentId::Search))),
            KeyCode::Char('e') => self.set_level(LogLevel::Error),
            KeyCode::Char('w') => self.set_level(LogLevel::Warning),
            KeyCode::Char('i') => self.set_level(LogLevel::Info),
            KeyCode::Char('d') => self.set_level(LogLevel::Debug),
            _ => (),
        };

        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Quit => self.token.cancel(),
            Action::Tick => {
                if self.live_mode.load(Ordering::Relaxed) {
                    self.throbber_state.calc_next();
                }
                if self.filter_pattern_changed {
                    let filter_pattern = self.filter_pattern.lock().unwrap();
                    let filter_pattern = filter_pattern.as_deref();
                    self.store.compute_view(filter_pattern);
                    self.filter_pattern_changed = false;
                }
                if self.level_changed {
                    self.token.cancel();
                    self.token = CancellationToken::new();
                    self.load_log()?;
                    self.level_changed = false;
                }
            }
            Action::SearchInputChanged(pattern) => {
                *self.filter_pattern.lock().unwrap() = pattern;
                self.filter_pattern_changed = true;
            }
            _ => {}
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        self.render_list(frame, area);
        self.render_scrollbar(frame, area);

        Ok(())
    }
}
