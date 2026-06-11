use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use const_format::concatcp;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Tabs;
use ratatui::{Frame, symbols};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::action::Action;
use crate::api::Api;
use crate::components::{Component, ComponentId, TABS};
use crate::config::Config;
use crate::utils::symbols::{SUPERSCRIPT, arrow};
use crate::version_update::SharedVersionUpdateState;
use crate::widgets::shortcut::{Fragment, Shortcut};

static TABS_FULL_WIDTH: LazyLock<u16> = LazyLock::new(|| {
    let len = TABS.len();
    let (superscript_width, padding_len) = (1usize, 3usize);

    let tabs_width: usize = TABS.iter().map(|id| superscript_width + id.full_name().len()).sum();
    let padding_width = len.saturating_sub(1) * padding_len;

    (tabs_width + padding_width) as u16
});

const RELEASE_CHECK_INTERVAL: Duration = Duration::from_hours(12);

pub struct HeaderComponent {
    selected: usize,

    api: Option<Arc<Api>>,
    config: Option<Arc<Config>>,
    version: Arc<Mutex<Option<String>>>,
    update_state: SharedVersionUpdateState,
    release_checker: Option<JoinHandle<()>>,
}

impl HeaderComponent {
    pub fn new(update_state: SharedVersionUpdateState) -> Self {
        Self {
            selected: Self::component_index(ComponentId::default()),
            api: None,
            config: None,
            version: Default::default(),
            update_state,
            release_checker: None,
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
                    *version.lock().unwrap() = Some(v.to_string());
                    Ok(())
                }
                Err(e) => {
                    error!(error = ?e, "Failed to load version");
                    Err(e)
                }
            }
        })?;
        Ok(())
    }

    fn start_release_checker(&mut self) -> anyhow::Result<()> {
        if self.release_checker.is_some() {
            return Ok(());
        }

        let Some(api) = self.api.as_ref().map(Arc::clone) else {
            return Ok(());
        };
        let Some(mihomo_repo) = self.config.as_ref().map(|config| config.mihomo_repo.clone())
        else {
            return Ok(());
        };
        let update_state = self.update_state.clone();
        let handle = tokio::task::Builder::new().name("release-checker").spawn(async move {
            loop {
                if let Err(e) = update_state.refresh(&api, &mihomo_repo).await {
                    warn!(error = ?e, "Failed to check release updates");
                }
                tokio::time::sleep(RELEASE_CHECK_INTERVAL).await;
            }
        })?;
        self.release_checker = Some(handle);
        Ok(())
    }

    fn build_marker() -> Span<'static> {
        Span::styled(concatcp!(arrow::UP, " "), Style::default().fg(Color::LightYellow))
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
        let version = {
            let guard = self.version.lock().unwrap();
            guard.as_deref().unwrap_or("-").to_string()
        };
        let availability = self.update_state.is_available();
        let mut spans = Vec::with_capacity(8);
        // mihomo core version
        spans.push(Span::styled(format!("[ {} ", version), Style::default().fg(Color::Blue)));
        if availability.core {
            spans.push(Self::build_marker())
        }
        // version separator
        spans.push(Span::raw(concatcp!(symbols::DOT, " ")));
        // tui version
        spans.push(Span::styled(
            concatcp!(env!("CARGO_PKG_VERSION"), " "),
            Style::default().fg(Color::LightCyan),
        ));
        if availability.app {
            spans.push(Self::build_marker())
        }
        spans.push(Fragment::hl("C-u").into_span(None));
        spans.push(Span::styled("]", Style::default().fg(Color::Blue)));

        let line = Line::from(spans).alignment(Alignment::Right);
        frame.render_widget(line, rect);
    }
}

impl Drop for HeaderComponent {
    fn drop(&mut self) {
        if let Some(handle) = self.release_checker.take() {
            handle.abort();
        }
    }
}

impl Component for HeaderComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Header
    }

    fn init(&mut self, api: Arc<Api>) -> anyhow::Result<()> {
        self.api = Some(Arc::clone(&api));
        let _ = self.start_release_checker();
        self.load_version(api)
    }

    fn register_config_handler(&mut self, config: Arc<Config>) -> anyhow::Result<()> {
        self.config = Some(config);
        let _ = self.start_release_checker();

        Ok(())
    }

    fn update(&mut self, action: Action) -> anyhow::Result<Option<Action>> {
        match action {
            Action::TabSwitch(to) => self.selected = Self::component_index(to),
            Action::CoreVersionUpdated(version) => {
                *self.version.lock().unwrap() = Some(version.to_string())
            }
            _ => (),
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
