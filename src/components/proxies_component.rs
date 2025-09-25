use std::sync::{Arc, RwLock};

use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::Color;
use ratatui::symbols::{bar, line};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Scrollbar, ScrollbarOrientation};
use tracing::{info, warn};

use crate::action::Action;
use crate::api::Api;
use crate::components::proxies::{LatencyQuality, Proxies, ProxyView};
use crate::components::{Component, ComponentId};
use crate::utils::symbols::arrow;
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT};
use crate::widgets::scrollbar::ScrollState;

const CARD_HEIGHT: u16 = 4;
const CARDS_PER_ROW: u16 = 2;

#[derive(Debug)]
pub struct ProxiesComponent {
    api: Option<Arc<Api>>,

    store: Arc<RwLock<Proxies>>,
    selected: Option<usize>,
    scroll_state: ScrollState,
}

impl Default for ProxiesComponent {
    fn default() -> Self {
        Self {
            api: None,
            store: Default::default(),
            selected: None,
            scroll_state: ScrollState::new(CARDS_PER_ROW as usize),
        }
    }
}

impl ProxiesComponent {
    fn load_proxies(&mut self) -> Result<()> {
        info!("Loading proxies");
        let api = Arc::clone(self.api.as_ref().unwrap());
        let store = Arc::clone(&self.store);

        tokio::task::Builder::new().name("proxies-loader").spawn(async move {
            // match tokio::try_join!(api.get_proxies(), api.get_providers()) {
            match api.get_proxies().await {
                Ok(proxies) => store.write().unwrap().push(proxies.proxies),
                Err(e) => warn!("Failed to get proxies: {e}"),
            }
        })?;
        Ok(())
    }

    fn render_scrollbar(&mut self, frame: &mut Frame, area: Rect) {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .track_symbol(Some(line::VERTICAL))
            .begin_symbol(Some(arrow::UP))
            .end_symbol(Some(arrow::DOWN));
        frame.render_stateful_widget(scrollbar, area, &mut self.scroll_state.state);
    }

    fn quality_stats_line(proxy: &'_ ProxyView, width: u16, total: usize) -> Line<'_> {
        let mut segments: Vec<(u16, f64)> = proxy
            .quality_stats
            .iter()
            .map(|&v| {
                let exact = v as f64 * width as f64 / total as f64;
                (exact.floor() as u16, exact.fract())
            })
            .collect();

        for _ in 0..width - segments.iter().map(|(n, _)| *n).sum::<u16>() {
            let seg = segments.iter_mut().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).unwrap();

            seg.0 += 1;
            seg.1 = 0.0;
        }

        segments
            .into_iter()
            .enumerate()
            .map(|(i, (c, _))| {
                Span::styled(
                    bar::THREE_EIGHTHS.repeat(c as usize),
                    LatencyQuality::try_from(i).unwrap().color(),
                )
            })
            .collect()
    }

    fn render_proxy(proxy: &ProxyView, selected: bool, frame: &mut Frame, area: Rect) {
        let title_line = Line::from(vec![
            Span::styled(proxy.proxy.name.as_str(), Color::White),
            Span::raw(" ("),
            Span::styled(
                format!("{}", proxy.proxy.all.as_ref().map_or(0, Vec::len)),
                Color::LightCyan,
            ),
            Span::raw(")"),
        ]);
        let (border_type, border_color) = if selected {
            (BorderType::Thick, Color::Cyan)
        } else {
            (BorderType::Rounded, Color::DarkGray)
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(border_type)
            .border_style(border_color)
            .title(title_line);
        let mut lines = vec![Line::from(vec![Span::raw(&proxy.proxy.r#type)])];
        if let Some(selected) = proxy.proxy.selected.as_ref() {
            lines[0].push_span(Span::styled(" > ", Color::DarkGray));
            lines[0].push_span(Span::styled(selected.as_str(), Color::Cyan));
        }

        let children = proxy.proxy.all.as_ref().map(|v| v.len()).unwrap_or(0);
        if children > 0 {
            let selected_delay = Span::styled(
                proxy.delay.map(|v| format!("{}", v)).unwrap_or("-".to_string()),
                LatencyQuality::from_delay(proxy.delay).color(),
            );
            let width = area.width - 10;
            let mut stats: Line = Self::quality_stats_line(proxy, width, children);
            stats.push_span(Span::raw(" ".repeat(10 - 2 - selected_delay.width())));
            stats.push_span(selected_delay);
            lines.push(stats);
        }
        let para = Paragraph::new(lines).block(block);
        frame.render_widget(para, area);
    }

    fn render_proxies(&mut self, frame: &mut Frame, outer: Rect) {
        let proxies = self.store.read().unwrap().view();

        let title_line = Line::from(vec![
            Span::raw(TOP_TITLE_LEFT),
            Span::raw("proxies ("),
            Span::styled(format!("{}", proxies.len()), Color::LightCyan),
            Span::raw(")"),
            Span::raw(TOP_TITLE_RIGHT),
        ]);
        let block = Block::bordered().border_type(BorderType::Rounded).title(title_line);
        let area = block.inner(outer);
        frame.render_widget(block, outer);

        let visible_cards = (area.height / CARD_HEIGHT) * CARDS_PER_ROW;
        self.scroll_state.length(proxies.len(), visible_cards as usize);

        let visible = &proxies[self.scroll_state.pos()..self.scroll_state.end_pos()];
        for (i, pair) in visible.chunks(CARDS_PER_ROW as usize).enumerate() {
            let y = area.y + (i as u16 * CARD_HEIGHT);
            if y >= area.y + area.height {
                break; // Don't render outside the area
            }

            let row_area = Rect { x: area.x, y, width: area.width, height: CARD_HEIGHT };
            let col_chunks =
                Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).split(row_area);

            for (col_idx, proxy) in pair.iter().enumerate() {
                let idx = self.scroll_state.pos() + i * CARDS_PER_ROW as usize + col_idx;
                let selected = self.selected.is_some_and(|v| v == idx);
                Self::render_proxy(proxy, selected, frame, col_chunks[col_idx]);
            }
        }
    }

    fn next(&mut self, step: usize) {
        let selected = self
            .selected
            .map(|v| v.saturating_add(step).min(self.scroll_state.content_length() - 1))
            .unwrap_or_default();
        self.selected = Some(selected);
        if selected >= self.scroll_state.end_pos() {
            self.scroll_state.next();
        }
    }

    fn prev(&mut self, step: usize) {
        let selected = self.selected.map(|v| v.saturating_sub(step)).unwrap_or_default();
        self.selected = Some(selected);
        if selected < self.scroll_state.pos() {
            self.scroll_state.prev();
        }
    }
}

impl Component for ProxiesComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Proxies
    }

    fn init(&mut self, api: Arc<Api>) -> Result<()> {
        self.api = Some(api);
        self.load_proxies()?;
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Char('g') => {
                if self.selected.is_some() {
                    self.selected = Some(0)
                }
                self.scroll_state.first();
            }
            KeyCode::Char('G') => {
                if self.selected.is_some() {
                    self.selected = Some(self.scroll_state.content_length() - 1)
                }
                self.scroll_state.last();
            }
            KeyCode::Char('j') | KeyCode::Down => self.next(2),
            KeyCode::Char('k') | KeyCode::Up => self.prev(2),
            KeyCode::Char('h') | KeyCode::Left => self.prev(1),
            KeyCode::Char('l') | KeyCode::Right => self.next(1),
            _ => (),
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        self.render_proxies(frame, area);
        self.render_scrollbar(frame, area.inner(Margin::new(0, 1)));

        Ok(())
    }
}
