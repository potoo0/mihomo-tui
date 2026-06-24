use std::ops::Range;
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

const TAB_SUPERSCRIPT_WIDTH: u16 = 1;
const TAB_PADDING_WIDTH: u16 = 2;
const TAB_DIVIDER_WIDTH: u16 = 1;

static TABS_FULL_WIDTH: LazyLock<u16> = LazyLock::new(|| {
    TABS.iter().map(|id| tab_width(id.full_name())).sum::<u16>() + divider_width(TABS.len())
});

static TAB_SHORT_WIDTHS: LazyLock<Vec<u16>> = LazyLock::new(|| {
    TABS.iter().map(|id| tab_width(id.short_name().unwrap_or_else(|| id.full_name()))).collect()
});

static TABS_SHORT_WIDTH: LazyLock<u16> =
    LazyLock::new(|| TAB_SHORT_WIDTHS.iter().sum::<u16>() + divider_width(TAB_SHORT_WIDTHS.len()));

const RELEASE_CHECK_INTERVAL: Duration = Duration::from_hours(12);

#[derive(Debug, Clone, Copy, PartialEq)]
enum TabNameMode {
    Full,
    Short,
}

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
        let (mode, range) = visible_tabs(rect.width, self.selected);
        let tabs: Vec<_> = TABS[range.clone()]
            .iter()
            .enumerate()
            .map(|(offset, cid)| {
                let i = range.start + offset;
                Shortcut::new(vec![
                    // TODO: Use proper superscript for index > 9
                    Fragment::hl(SUPERSCRIPT[i + 1]),
                    Fragment::raw(tab_name(*cid, mode)),
                ])
                .into_spans(None)
            })
            .collect();
        let tabs = Tabs::new(tabs).select(self.selected - range.start).divider("|");
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

fn divider_width(tab_count: usize) -> u16 {
    tab_count.saturating_sub(1) as u16 * TAB_DIVIDER_WIDTH
}

fn tab_width(name: &str) -> u16 {
    TAB_SUPERSCRIPT_WIDTH + name.len() as u16 + TAB_PADDING_WIDTH
}

fn tab_name(id: ComponentId, mode: TabNameMode) -> &'static str {
    match mode {
        TabNameMode::Full => id.full_name(),
        TabNameMode::Short => id.short_name().unwrap_or_else(|| id.full_name()),
    }
}

fn visible_tabs(available_width: u16, selected: usize) -> (TabNameMode, Range<usize>) {
    if available_width >= *TABS_FULL_WIDTH {
        return (TabNameMode::Full, 0..TABS.len());
    }
    if available_width >= *TABS_SHORT_WIDTH {
        return (TabNameMode::Short, 0..TABS.len());
    }

    (
        TabNameMode::Short,
        visible_tab_range(
            TAB_SHORT_WIDTHS.as_slice(),
            selected,
            TABS_SHORT_WIDTH.saturating_sub(available_width),
        ),
    )
}

fn visible_tab_range(widths: &[u16], selected: usize, mut overflow_width: u16) -> Range<usize> {
    if selected >= widths.len() {
        return 0..0;
    };

    let mut start = 0;
    let mut end = widths.len();

    // The full short tab list is only slightly wider in normal narrow layouts,
    // so trimming from the full range usually takes fewer rounds than expanding
    // from the selected tab.
    while overflow_width > 0 && end - start > 1 {
        let can_trim_left = start < selected;
        let can_trim_right = end > selected + 1;

        let trim_left = match (can_trim_left, can_trim_right) {
            (false, false) => break,
            (true, false) => true,
            (false, true) => false,
            (true, true) => {
                let left_count = selected - start;
                let right_count = end - selected - 1;

                left_count >= right_count
            }
        };

        let trimmed_width = if trim_left {
            start += 1;
            widths[start - 1]
        } else {
            end -= 1;
            widths[end]
        } + TAB_DIVIDER_WIDTH;

        overflow_width = overflow_width.saturating_sub(trimmed_width);
    }

    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn width_for(widths: &[u16], range: Range<usize>) -> u16 {
        widths[range.clone()].iter().sum::<u16>() + divider_width(range.len())
    }

    #[test]
    fn visible_tab_range_cases() {
        #[derive(Debug)]
        struct Case {
            widths: Vec<u16>,
            selected: usize,
            overflow_width: u16,
            expected: Range<usize>,
        }

        let cases = [
            Case { widths: vec![3, 3, 3], selected: 1, overflow_width: 0, expected: 0..3 },
            Case { widths: vec![7, 7, 7], selected: 1, overflow_width: 100, expected: 1..2 },
            Case { widths: vec![5; 8], selected: 5, overflow_width: 30, expected: 4..7 },
            Case { widths: vec![5; 8], selected: 6, overflow_width: 30, expected: 5..8 },
        ];

        for case in cases {
            assert_eq!(
                visible_tab_range(&case.widths, case.selected, case.overflow_width),
                case.expected,
                "{case:?}"
            );
        }
    }

    #[test]
    fn visible_tabs_mode_cases() {
        let cases = [
            (*TABS_FULL_WIDTH, TabNameMode::Full, 0..TABS.len()),
            (*TABS_SHORT_WIDTH, TabNameMode::Short, 0..TABS.len()),
        ];

        for (available_width, expected_mode, expected_range) in cases {
            let (mode, range) = visible_tabs(available_width, 0);

            assert_eq!(mode, expected_mode);
            assert_eq!(range, expected_range);
        }
    }

    #[test]
    fn visible_tabs_clips_short_tabs_when_short_width_does_not_fit() {
        let selected = 6;
        let (mode, range) = visible_tabs(*TABS_SHORT_WIDTH - 1, selected);

        assert_eq!(mode, TabNameMode::Short);
        assert!(range.contains(&selected));
        assert!(range.end < TABS.len() || range.start > 0);
        assert!(width_for(TAB_SHORT_WIDTHS.as_slice(), range) < *TABS_SHORT_WIDTH);
    }
}
