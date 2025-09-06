use std::sync::{Arc, OnceLock};

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Tabs;
use ratatui::{Frame, symbols};
use tracing::{error, info};

use crate::action::Action;
use crate::api::Api;
use crate::components::highlight::{Fragment, HighlightedLine};
use crate::components::{AppState, Component, ComponentId, TABS};
use crate::models::Version;
use crate::utils::symbols::SUPERSCRIPT;

#[derive(Default)]
pub struct HeaderComponent {
    main_component: ComponentId,

    api: Option<Arc<Api>>,
    version: Arc<OnceLock<Version>>,
}

impl HeaderComponent {
    fn load_version(&mut self, api: Arc<Api>) -> color_eyre::Result<()> {
        info!("Loading version");
        let version = Arc::clone(&self.version);
        tokio::task::Builder::new().name("version-loader").spawn(async move {
            match api.get_version().await {
                Ok(v) => {
                    let _ = version.set(v);
                    Ok(())
                }
                Err(e) => {
                    error!("Failed to load version: {}", e);
                    Err(e)
                }
            }
        })?;
        Ok(())
    }

    fn render_tab(&self, frame: &mut Frame, rect: Rect) {
        let tabs: Vec<Line> = TABS
            .iter()
            .enumerate()
            .map(|(i, cid)| {
                HighlightedLine::new(vec![
                    Fragment::Hl(SUPERSCRIPT[i + 1 % SUPERSCRIPT.len()]),
                    Fragment::RawOwned(cid.to_string()),
                ])
                .into()
            })
            .collect();
        let selected_index = TABS.iter().position(|cid| *cid == self.main_component).unwrap_or(0);
        let tabs = Tabs::new(tabs).select(selected_index).divider("|");
        frame.render_widget(tabs, rect);
    }

    fn render_version(&self, frame: &mut Frame, rect: Rect) {
        let version = self.version.get().map(ToString::to_string).unwrap_or("-".to_string());
        let line = Line::from(vec![
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
        .alignment(Alignment::Right);
        frame.render_widget(line, rect);
    }
}

impl Component for HeaderComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Header
    }

    fn init(&mut self, api: Arc<Api>) -> color_eyre::Result<()> {
        self.api = Some(Arc::clone(&api));
        self.load_version(api)
    }

    fn update(&mut self, action: Action) -> color_eyre::Result<Option<Action>> {
        if let Action::TabSwitch(to) = action {
            self.main_component = to;
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, _state: &AppState) -> color_eyre::Result<()> {
        let chunks = Layout::horizontal([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(area);

        self.render_tab(frame, chunks[0]);
        self.render_version(frame, chunks[1]);
        Ok(())
    }
}
