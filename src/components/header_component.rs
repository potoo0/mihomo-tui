use std::sync::{Arc, LazyLock, OnceLock};

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Tabs;
use ratatui::{Frame, symbols};
use tracing::{error, info};

use crate::action::Action;
use crate::api::Api;
use crate::components::{Component, ComponentId, TABS};
use crate::models::Version;
use crate::utils::symbols::SUPERSCRIPT;
use crate::widgets::shortcut::{Fragment, Shortcut};

static TABS_FULL_WIDTH: LazyLock<u16> = LazyLock::new(|| {
    let len = TABS.len();
    let (superscript_width, padding_len) = (1usize, 3usize);

    let tabs_width: usize = TABS.iter().map(|id| superscript_width + id.full_name().len()).sum();
    let padding_width = len.saturating_sub(1) * padding_len;

    (tabs_width + padding_width) as u16
});

#[derive(Default)]
pub struct HeaderComponent {
    selected: usize,

    api: Option<Arc<Api>>,
    version: Arc<OnceLock<Version>>,
}

impl HeaderComponent {
    pub fn new() -> Self {
        Self {
            selected: Self::component_index(ComponentId::default()),
            api: None,
            version: Arc::new(OnceLock::new()),
        }
    }

    fn component_index(id: ComponentId) -> usize {
        TABS.iter().position(|c| *c == id).unwrap_or(0)
    }

    fn load_version(&mut self, api: Arc<Api>) -> anyhow::Result<()> {
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
        let compact_mode = rect.width < *TABS_FULL_WIDTH;
        let tabs: Vec<Line> = TABS
            .iter()
            .enumerate()
            .map(|(i, cid)| {
                let name = if compact_mode && i != self.selected {
                    cid.short_name().unwrap_or_else(|| cid.full_name())
                } else {
                    cid.full_name()
                };
                Shortcut::new(vec![
                    // TODO: Use proper superscript for index > 9
                    Fragment::hl(SUPERSCRIPT[i + 1]),
                    Fragment::raw(name),
                ])
                .into()
            })
            .collect();
        let tabs = Tabs::new(tabs).select(self.selected).divider("|");
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

    fn init(&mut self, api: Arc<Api>) -> anyhow::Result<()> {
        self.api = Some(Arc::clone(&api));
        self.load_version(api)
    }

    fn update(&mut self, action: Action) -> anyhow::Result<Option<Action>> {
        if let Action::TabSwitch(to) = action {
            self.selected = Self::component_index(to);
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> anyhow::Result<()> {
        let chunks = Layout::horizontal([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(area);

        self.render_tab(frame, chunks[0]);
        self.render_version(frame, chunks[1]);
        Ok(())
    }
}
