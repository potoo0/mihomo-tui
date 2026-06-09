use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::Duration;

use const_format::concatcp;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Tabs;
use ratatui::{Frame, symbols};
use semver::Version as SemverVersion;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::action::Action;
use crate::api::{Api, GithubApi};
use crate::components::{Component, ComponentId, TABS};
use crate::config::Config;
use crate::models::Version;
use crate::utils::symbols::{SUPERSCRIPT, arrow};
use crate::widgets::shortcut::{Fragment, Shortcut};

static TABS_FULL_WIDTH: LazyLock<u16> = LazyLock::new(|| {
    let len = TABS.len();
    let (superscript_width, padding_len) = (1usize, 3usize);

    let tabs_width: usize = TABS.iter().map(|id| superscript_width + id.full_name().len()).sum();
    let padding_width = len.saturating_sub(1) * padding_len;

    (tabs_width + padding_width) as u16
});

const RELEASE_CHECK_INTERVAL: Duration = Duration::from_hours(12);
const RELEASE_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Default)]
enum UpdateStatus {
    #[default]
    Unknown,
    UpToDate,
    Available,
}

#[derive(Default)]
pub struct HeaderComponent {
    selected: usize,

    api: Option<Arc<Api>>,
    config: Option<Arc<Config>>,
    version: Arc<OnceLock<Version>>,
    app_update_status: Arc<Mutex<UpdateStatus>>,
    mihomo_update_status: Arc<Mutex<UpdateStatus>>,
    release_checker: Option<JoinHandle<()>>,
}

impl HeaderComponent {
    pub fn new() -> Self {
        Self {
            selected: Self::component_index(ComponentId::default()),
            api: None,
            config: None,
            version: Default::default(),
            app_update_status: Default::default(),
            mihomo_update_status: Default::default(),
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
                    let _ = version.set(v);
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
        let app_update_status = Arc::clone(&self.app_update_status);
        let mihomo_update_status = Arc::clone(&self.mihomo_update_status);
        let handle = tokio::task::Builder::new().name("release-checker").spawn(async move {
            let github_api = match GithubApi::new(RELEASE_CHECK_TIMEOUT) {
                Ok(github_api) => github_api,
                Err(e) => {
                    error!(error = ?e, "Failed to build GitHub release checker client");
                    return;
                }
            };

            loop {
                if let Err(e) =
                    Self::refresh_app_update_status(&github_api, &app_update_status).await
                {
                    warn!(error = ?e, "Failed to check latest mihomo-tui release");
                }
                if let Err(e) = Self::refresh_mihomo_update_status(
                    &api,
                    &github_api,
                    &mihomo_repo,
                    &mihomo_update_status,
                )
                .await
                {
                    warn!(error = ?e, "Failed to check latest mihomo release");
                }
                tokio::time::sleep(RELEASE_CHECK_INTERVAL).await;
            }
        })?;
        self.release_checker = Some(handle);
        Ok(())
    }

    async fn refresh_app_update_status(
        github_api: &GithubApi,
        update_status: &Mutex<UpdateStatus>,
    ) -> anyhow::Result<()> {
        let Some(latest_tag) = github_api.latest_release_tag(env!("CARGO_PKG_REPOSITORY")).await?
        else {
            warn!(
                repository = env!("CARGO_PKG_REPOSITORY"),
                "Repository URL is not a GitHub repository"
            );
            return Ok(());
        };
        debug!(repo = env!("CARGO_PKG_REPOSITORY"), latest_tag, "Fetched latest app release tag");

        let status = update_status_from_versions(env!("CARGO_PKG_VERSION"), &latest_tag)?;
        debug!(
            repo = env!("CARGO_PKG_REPOSITORY"),
            current = env!("CARGO_PKG_VERSION"),
            ?status,
            "Parsed latest app version"
        );

        let mut writable = update_status.lock().unwrap();
        *writable = status;
        Ok(())
    }

    async fn refresh_mihomo_update_status(
        api: &Api,
        github_api: &GithubApi,
        mihomo_repo: &str,
        update_status: &Mutex<UpdateStatus>,
    ) -> anyhow::Result<()> {
        let Some(latest_tag) = github_api.latest_release_tag(mihomo_repo).await? else {
            warn!(repository = mihomo_repo, "Mihomo repository is not a GitHub repository");
            return Ok(());
        };
        debug!(repo = mihomo_repo, latest_tag, "Fetched latest Mihomo release tag");

        let current = api.get_version().await?;
        let status = update_status_from_versions(&current.version, &latest_tag)?;
        debug!(
            repo = mihomo_repo,
            current = current.version,
            ?status,
            "Parsed latest Mihomo version"
        );

        let mut writable = update_status.lock().unwrap();
        *writable = status;
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
        let mut spans = Vec::with_capacity(6);
        // mihomo core version
        spans.push(Span::styled(format!("[ {} ", version), Style::default().fg(Color::Blue)));
        let marker = {
            let guard = self.mihomo_update_status.lock().unwrap();
            guard.marker()
        };
        if let Some(marker) = marker {
            spans.push(marker);
        }
        // version separator
        spans.push(Span::raw(concatcp!(symbols::DOT, " ")));
        // tui version
        spans.push(Span::styled(
            concatcp!(env!("CARGO_PKG_VERSION"), " "),
            Style::default().fg(Color::LightCyan),
        ));
        let marker = {
            let guard = self.app_update_status.lock().unwrap();
            guard.marker()
        };
        if let Some(marker) = marker {
            spans.push(marker);
        }
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

impl UpdateStatus {
    fn marker(&self) -> Option<Span<'static>> {
        match self {
            Self::Available => Some(Span::styled(
                concatcp!(arrow::UP, " "),
                Style::default().fg(Color::LightYellow),
            )),
            Self::Unknown | Self::UpToDate => None,
        }
    }
}

fn update_status_from_versions(current: &str, latest_tag: &str) -> anyhow::Result<UpdateStatus> {
    let current = current.trim_start_matches('v');
    let latest = latest_tag.trim_start_matches('v');
    let current = SemverVersion::parse(current)?;
    let latest_version = SemverVersion::parse(latest)?;

    if latest_version > current { Ok(UpdateStatus::Available) } else { Ok(UpdateStatus::UpToDate) }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[ignore]
    #[test]
    fn test_lock() {
        let update_status: Mutex<UpdateStatus> = Default::default();
        *update_status.lock().unwrap() = UpdateStatus::Available;
        let marker = {
            let guard = update_status.lock().unwrap();
            guard.marker()
        };
        if let Some(marker) = marker {
            // NOTE: deadlock
            // if let Some(marker) = update_status.lock().unwrap().clone().marker() {
            println!("marker: {}", marker);
            let guard = update_status.lock().unwrap();
            println!("guard: {:?}", guard);
        }
        println!("end update_status: {:?}", update_status.lock().unwrap());
    }

    #[test]
    fn update_status_from_versions_matches_release_version() {
        enum Expected {
            Available,
            UpToDate,
            Error,
        }

        let cases = vec![
            ("0.4.2", "v0.4.3", Expected::Available),
            ("v1.19.27", "v1.19.28", Expected::Available),
            ("0.4.2", "0.4.2", Expected::UpToDate),
            ("0.4.2", "0.4.1", Expected::UpToDate),
            ("0.4.2", "nightly", Expected::Error),
        ];

        for (current, latest_tag, expected) in cases {
            let status = update_status_from_versions(current, latest_tag);
            match expected {
                Expected::Available => assert!(matches!(status.unwrap(), UpdateStatus::Available)),
                Expected::UpToDate => assert!(matches!(status.unwrap(), UpdateStatus::UpToDate)),
                Expected::Error => assert!(status.is_err()),
            }
        }
    }
}
