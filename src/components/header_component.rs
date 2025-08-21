use crate::action::Action;
use crate::components::{Component, ComponentId, SUPERSCRIPT_NUMS, TABS};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Tabs;
use ratatui::Frame;
use tokio::sync::mpsc::UnboundedSender;
use tracing::debug;

#[derive(Default)]
pub struct HeaderComponent {
    main_component: ComponentId,
    action_tx: Option<UnboundedSender<Action>>,
}

impl HeaderComponent {
    pub fn foo() -> Self {
        todo!()
    }

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

    fn version_widget(&self) -> Line<'_> {
        Line::from(vec![
            Span::styled("meta", Style::default().add_modifier(Modifier::BOLD)),
            Span::from(" v1.2.3 "),
            Span::styled("tui", Style::default().fg(Color::DarkGray)),
            Span::from(format!(" {}", env!("CARGO_PKG_VERSION"))),
        ])
    }
}

impl Component for HeaderComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Header
    }

    fn update(&mut self, action: Action) -> eyre::Result<Option<Action>> {
        match action {
            Action::TabSwitch { from, to } => {
                self.main_component = to;
                debug!("Switched tab from {} to {}", from, to);
            }
            _ => {}
        }
        Ok(None)
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> eyre::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> eyre::Result<()> {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(area);

        frame.render_widget(self.tab_widget(), chunks[0]);
        frame.render_widget(self.version_widget(), chunks[1]);

        Ok(())
    }
}
