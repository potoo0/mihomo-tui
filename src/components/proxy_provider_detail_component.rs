use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::prelude::{Color, Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

use crate::action::Action;
use crate::components::proxy_setting::get_proxy_setting;
use crate::components::{Component, ComponentId};
use crate::models::provider::ProxyProvider;
use crate::models::proxy::Proxy;
use crate::utils::symbols::arrow;
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT, popup_area, space_between};
use crate::widgets::scrollable_navigator::ScrollableNavigator;
use crate::widgets::shortcut::{Fragment, Shortcut};

const CARD_HEIGHT: u16 = 3;
const CARD_WIDTH: u16 = 25;

#[derive(Debug, Default)]
pub struct ProxyProviderDetailComponent {
    show: bool,

    store: Option<Arc<ProxyProvider>>,
    navigator: ScrollableNavigator,
}

impl ProxyProviderDetailComponent {
    pub fn show(&mut self, provider: Arc<ProxyProvider>) {
        self.show = true;
        self.store = Some(provider);
        self.navigator.focused = None;
        self.navigator.scroller.position(0);
    }

    pub fn hide(&mut self) {
        self.show = false;
        self.store = None;
    }

    fn title_line(&'_ self) -> Line<'_> {
        let Some(provider) = self.store.as_ref() else {
            return Line::raw("-");
        };
        Line::from(vec![
            Span::raw(TOP_TITLE_LEFT),
            Span::styled(provider.name.as_str(), Color::White),
            Span::raw(" ("),
            Span::styled(format!("{}", provider.proxies.len()), Color::LightCyan),
            Span::raw(") - "),
            Span::raw(provider.vehicle_type.as_str()),
            Span::raw(TOP_TITLE_RIGHT),
        ])
    }

    fn render_card(proxy: &Proxy, focused: bool, frame: &mut Frame, area: Rect) {
        let (border_type, border_color) = if focused {
            (BorderType::Thick, Color::Cyan)
        } else {
            (BorderType::Rounded, Color::DarkGray)
        };
        let block = Block::bordered()
            .border_type(border_type)
            .border_style(border_color)
            .title_top(Span::raw(proxy.name.as_str()));

        let threshold = get_proxy_setting().read().unwrap().threshold;
        let para = Paragraph::new(space_between(
            area.width - 2, // minus border
            Span::raw(proxy.r#type.as_str()),
            proxy.latency.as_span(threshold),
        ))
        .block(block);
        frame.render_widget(para, area);
    }

    fn render_cards(&mut self, frame: &mut Frame, area: Rect) {
        let Some(provider) = self.store.as_ref() else {
            return;
        };

        let cols = (area.width / CARD_WIDTH).max(1) as usize;
        let col_chunks =
            Layout::horizontal((0..cols).map(|_| Constraint::Min(CARD_WIDTH))).split(area);
        self.navigator
            .step(cols)
            .length(provider.proxies.len(), ((area.height / CARD_HEIGHT) as usize) * cols);
        self.navigator.iter_visible(&provider.proxies, CARD_HEIGHT, col_chunks).for_each(
            |(proxy, focused, rect)| {
                Self::render_card(proxy, focused, frame, rect);
            },
        );
    }
}

impl Component for ProxyProviderDetailComponent {
    fn id(&self) -> ComponentId {
        ComponentId::ProxyProviderDetail
    }

    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new(vec![
                Fragment::hl(arrow::UP),
                Fragment::raw("/"),
                Fragment::hl(arrow::LEFT),
                Fragment::raw(" nav "),
                Fragment::hl(arrow::RIGHT),
                Fragment::raw("/"),
                Fragment::hl(arrow::DOWN),
            ]),
            Shortcut::new(vec![Fragment::hl("g"), Fragment::raw(" jump "), Fragment::hl("G")]),
            Shortcut::new(vec![
                Fragment::hl("PgUp"),
                Fragment::raw(" page "),
                Fragment::hl("PgDn"),
            ]),
            Shortcut::new(vec![Fragment::raw("back "), Fragment::hl("Esc")]),
        ]
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> anyhow::Result<Option<Action>> {
        if self.navigator.handle_key_event(true, key) {
            return Ok(None);
        }
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(Some(Action::Quit));
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                self.hide();
                return Ok(Some(Action::Unfocus));
            }
            _ => (),
        }

        Ok(None)
    }

    fn update(&mut self, action: Action) -> anyhow::Result<Option<Action>> {
        if let Action::ProxyProviderDetail(p) = action {
            self.show(p)
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> anyhow::Result<()> {
        if !self.show {
            return Ok(());
        }

        let area = popup_area(area, 80, 80);
        frame.render_widget(Clear, area); // clears out the background
        // outer margin
        let area = area.inner(Margin::new(2, 1));

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Color::LightBlue)
            .title(self.title_line());
        let content_area = block.inner(area);
        frame.render_widget(block, area);

        self.render_cards(frame, content_area);
        self.navigator.render(frame, area.inner(Margin::new(0, 1)));

        Ok(())
    }
}
