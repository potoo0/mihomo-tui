use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Tabs;
use ratatui::{Frame, symbols};

use crate::action::Action;
use crate::components::{AppState, Component, ComponentId, SUPERSCRIPT_NUMS, TABS};

#[derive(Default)]
pub struct HeaderComponent {
    main_component: ComponentId,
}

impl HeaderComponent {
    fn tab_widget(&self) -> Tabs<'_> {
        let tabs: Vec<Line> = TABS
            .iter()
            .enumerate()
            .map(|(i, cid)| {
                let superscript = SUPERSCRIPT_NUMS[i + 1 % SUPERSCRIPT_NUMS.len()];
                Line::from(vec![
                    Span::styled(superscript, Style::default().fg(Color::Rgb(175, 95, 95))),
                    Span::from(cid.to_string()),
                ])
            })
            .collect();
        let selected_index = TABS
            .iter()
            .position(|cid| *cid == self.main_component)
            .unwrap_or(0);
        Tabs::new(tabs).select(selected_index).divider("|")
    }

    fn version_widget(&self, state: &AppState) -> Line<'_> {
        let version = state
            .version
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or("-".to_string());
        Line::from(vec![
            Span::styled(
                format!("[ {} {} ", version, symbols::DOT),
                Style::default().fg(Color::Blue),
            ),
            Span::styled(
                format!("{} ", env!("CARGO_PKG_VERSION")),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled("]", Style::default().fg(Color::Blue)),
        ])
        .alignment(Alignment::Right)
    }
}

impl Component for HeaderComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Header
    }

    fn update(&mut self, action: Action) -> color_eyre::Result<Option<Action>> {
        if let Action::TabSwitch(to) = action {
            self.main_component = to;
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, state: &AppState) -> color_eyre::Result<()> {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(area);

        frame.render_widget(self.tab_widget(), chunks[0]);
        frame.render_widget(self.version_widget(state), chunks[1]);

        Ok(())
    }
}
