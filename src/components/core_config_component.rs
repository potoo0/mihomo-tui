use std::fs::File;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::{Span, Stylize};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, BorderType, Paragraph};
use serde::Serialize;
use serde_json::{Serializer, Value};
use tempfile::{Builder, NamedTempFile};
use throbber_widgets_tui::{BRAILLE_SIX, Throbber, ThrobberState, WhichUse};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, info, warn};

use crate::action::Action;
use crate::api::Api;
use crate::components::{Component, ComponentId};
use crate::config::Config;
use crate::models::CoreConfig;
use crate::utils::editor::resolve_editor;
use crate::utils::json5_formatter::{Json5Formatter, collect_paths, extract_comments};
use crate::utils::symbols::arrow;
use crate::utils::text_ui::{dashed_title_line, top_title_line};
use crate::widgets::button::Button;
use crate::widgets::scrollbar::Scroller;
use crate::widgets::shortcut::{Fragment, Shortcut};

/// schema for core config JSON
const DEFAULT_SCHEMA: &str = include_str!("../../.config/core-config.schema.json");

/// Action button labels and constraints
const ACTIONS: [&str; 5] = ["Reload", "Restart", "Flush FakeIP", "Flush DNS", "Update GEO"];
const ACTION_CONSTRAINTS: [Constraint; ACTIONS.len()] = [Constraint::Min(1); ACTIONS.len()];

#[derive(Debug, Default)]
pub struct CoreConfigComponent {
    api: Option<Arc<Api>>,
    action_tx: Option<UnboundedSender<Action>>,
    config: Option<Arc<Config>>,

    active_pane: ActivePane,
    store: Arc<RwLock<String>>,
    editor_state: EditorState,
    modified: Arc<AtomicBool>,

    line_count: Arc<AtomicUsize>,
    scroller: Scroller,

    loading: Arc<AtomicBool>,
    throbber: ThrobberState,
}

/// Async task execution context (`'static + Send`).
/// Contains only shared, thread-safe state; no UI-only fields.
#[derive(Clone, Debug)]
struct TaskContext {
    api: Arc<Api>,
    store: Arc<RwLock<String>>,
    line_count: Arc<AtomicUsize>,
    modified: Arc<AtomicBool>,
    loading: Arc<AtomicBool>,
    app_config: Arc<Config>,
}

#[derive(Debug, Default)]
enum EditorState {
    #[default]
    Idle,
    Editing(NamedTempFile),
    SyncFailed,
}

#[derive(Copy, Clone, Debug, Default)]
enum ActivePane {
    #[default]
    Editor,
    Action(usize),
}

impl ActivePane {
    pub fn next(self, action_len: usize) -> Self {
        match self {
            ActivePane::Editor => ActivePane::Action(0),
            ActivePane::Action(i) if i + 1 < action_len => ActivePane::Action(i + 1),
            ActivePane::Action(_) => ActivePane::Editor,
        }
    }

    pub fn prev(self, action_len: usize) -> Self {
        match self {
            ActivePane::Editor => ActivePane::Action(action_len.saturating_sub(1)),
            ActivePane::Action(0) => ActivePane::Editor,
            ActivePane::Action(i) => ActivePane::Action(i - 1),
        }
    }
}

impl CoreConfigComponent {
    fn task_context(&self) -> TaskContext {
        TaskContext {
            api: Arc::clone(self.api.as_ref().unwrap()),
            store: Arc::clone(&self.store),
            line_count: Arc::clone(&self.line_count),
            modified: Arc::clone(&self.modified),
            loading: Arc::clone(&self.loading),
            app_config: Arc::clone(self.config.as_ref().unwrap()),
        }
    }

    fn load_core_config(&mut self) -> Result<()> {
        info!("Loading core config");
        let ctx = self.task_context();

        tokio::task::Builder::new().name("core-config-loader").spawn(async move {
            Self::refresh_core_config(ctx).await;
        })?;
        Ok(())
    }

    async fn refresh_core_config(ctx: TaskContext) {
        match ctx
            .api
            .get_core_config()
            .await
            .with_context(|| "failed to get core config from mihomo API")
            .and_then(|config| Self::pretty_print_core_config(&ctx, config))
        {
            Ok(config) => {
                ctx.line_count.store(config.lines().count(), Ordering::Relaxed);
                ctx.modified.store(false, Ordering::Relaxed);
                ctx.loading.store(false, Ordering::Relaxed);

                let mut writable = ctx.store.write().unwrap();
                *writable = config;
            }
            Err(e) => {
                error!(error = ?e, "load core config failed");
                ctx.loading.store(false, Ordering::Relaxed);
            }
        }
    }

    fn pretty_print_core_config(ctx: &TaskContext, config: CoreConfig) -> Result<String> {
        let paths = collect_paths(&config);
        let json_schema = Self::load_config_schema(ctx.app_config.as_ref()).unwrap_or_else(|err| {
            error!(error = ?err, "load core config schema failed, using empty schema");
            Value::Null
        });
        let comments = extract_comments(&json_schema);
        let formatter = Json5Formatter::new(b"  ", paths, &comments);

        // serialize with custom formatter
        let mut buf = Vec::with_capacity(1024);
        let mut ser = Serializer::with_formatter(&mut buf, formatter);
        config.serialize(&mut ser)?;

        String::from_utf8(buf).with_context(|| "failed to convert config to UTF-8")
    }

    fn load_config_schema(config: &Config) -> Result<Value> {
        match config.mihomo_config_schema.as_deref() {
            Some(path) => {
                info!("Loading core config schema from file: {:?}", path);
                let file = File::open(path).with_context(|| {
                    format!("failed to open core config schema file: {:?}", path)
                })?;
                serde_json::from_reader(file)
                    .with_context(|| format!("failed to parse core config schema file: {:?}", path))
            }
            None => serde_json::from_str(DEFAULT_SCHEMA)
                .with_context(|| "failed to parse builtin core config schema file"),
        }
    }

    fn edit_core_config(&mut self) -> Result<Option<Action>> {
        let mut file = Builder::new().prefix("mihomo_cfg").suffix(".json5").tempfile()?;
        {
            let store = Arc::clone(&self.store);
            let readable = store.read().unwrap();
            use std::io::Write;
            file.write_all(readable.as_bytes())?;
            file.flush()?;
        }
        let filepath = file.path().to_owned();
        let editor = resolve_editor();
        self.editor_state = EditorState::Editing(file);

        Ok(Some(Action::SpawnExternalEditor(editor, filepath)))
    }

    fn sync_core_config(&mut self) -> Result<()> {
        if let EditorState::Editing(temp_file) = &self.editor_state {
            let path = temp_file.path();
            // write back to store
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read edited core config file: {:?}", path))?;
            let modified = {
                let readable = self.store.read().unwrap();
                content != *readable
            };
            if modified {
                self.line_count.store(content.lines().count(), Ordering::Relaxed);
                self.scroller.first();
                let mut writable = self.store.write().unwrap();
                *writable = content;
            }
            info!("Core config edited and synced from file: {:?}", path);
            self.modified.store(modified, Ordering::Relaxed);
            self.editor_state = Default::default();
        }
        Ok(())
    }

    /// Submits the edited core configuration to the API.
    ///
    /// Skips the submission if a loading process is already in progress to avoid state conflicts.
    fn submit_core_config(&mut self) -> Result<()> {
        if self.loading.load(Ordering::Relaxed) {
            warn!("Operations are in progress, submission is skipped");
            return Ok(());
        }

        if !self.modified.load(Ordering::Relaxed) {
            return Ok(());
        }
        info!("Submitting updated core config...");

        // prepare content
        let content = {
            let readable = self.store.read().unwrap();
            let value: Value =
                json5::from_str(&readable).with_context(|| "failed to parse config as JSON5")?;
            serde_json::to_vec(&value)?
        };

        let ctx = self.task_context();
        let action_tx = self.action_tx.as_ref().unwrap().clone();

        ctx.loading.store(true, Ordering::Relaxed);
        tokio::task::Builder::new().name("core-config-submitter").spawn(async move {
            match ctx.api.update_core_config(content).await {
                Ok(_) => {
                    info!("Core config successfully submitted");
                    ctx.modified.store(false, Ordering::Relaxed);
                }
                Err(e) => {
                    error!(error = ?e, "Failed to submit core config to mihomo API");
                    let _ = action_tx.send(Action::Error(("Submit core config", e).into()));
                }
            }
            Self::refresh_core_config(ctx).await;
        })?;
        Ok(())
    }

    fn handle_action_button(&mut self, idx: usize) -> Result<()> {
        let action_name = match ACTIONS.get(idx) {
            Some(name) => *name,
            None => return Ok(()),
        };
        if self.loading.load(Ordering::Relaxed) {
            warn!("Operations are in progress, action '{}' is skipped", action_name);
            return Ok(());
        }

        info!("Triggering core action '{}'", action_name);
        let ctx = self.task_context();
        let action_tx = self.action_tx.as_ref().unwrap().clone();

        ctx.loading.store(true, Ordering::Relaxed);
        tokio::task::Builder::new().name("core-action-trigger").spawn(async move {
            let result = match idx {
                0 => ctx.api.reload_config().await,
                1 => ctx.api.restart().await,
                2 => ctx.api.flush_fake_ip_cache().await,
                3 => ctx.api.flush_dns_cache().await,
                4 => ctx.api.update_geo().await,
                _ => return,
            };
            match result {
                Ok(_) => info!("Core action '{}' completed successfully", action_name),
                Err(e) => {
                    error!(error = ?e, action = action_name, "Core action failed");
                    let _ = action_tx.send(Action::Error((action_name, e).into()));
                }
            }
            ctx.loading.store(false, Ordering::Relaxed);
        })?;
        Ok(())
    }

    fn handle_pane_switch(&mut self, key: KeyEvent) -> bool {
        let is_editor = matches!(self.active_pane, ActivePane::Editor);

        let switched = match key.code {
            KeyCode::Tab => {
                self.active_pane = self.active_pane.next(ACTIONS.len());
                true
            }
            KeyCode::BackTab => {
                self.active_pane = self.active_pane.prev(ACTIONS.len());
                true
            }
            _ => false,
        };

        // update shortcuts if pane switched between editor and action
        if switched && is_editor != matches!(self.active_pane, ActivePane::Editor) {
            self.action_tx.as_ref().unwrap().send(Action::Shortcuts(self.shortcuts())).unwrap();
        }
        switched
    }

    fn render_cfg_preview(&mut self, frame: &mut Frame, area: Rect) {
        self.scroller.length(
            self.line_count.load(Ordering::Relaxed),
            area.height.saturating_sub(2) as usize,
        );
        let title = if self.modified.load(Ordering::Relaxed) {
            Span::styled(" core config * ", Style::default().fg(Color::Yellow))
        } else {
            Span::raw(" core config ")
        };
        let block_style = match (self.active_pane, &self.editor_state) {
            (ActivePane::Editor, _) => Style::default().fg(Color::LightBlue),
            (_, EditorState::SyncFailed) => Style::default().fg(Color::Red),
            _ => Style::default(),
        };

        // hold read lock while rendering: `content` borrows from `store`
        {
            let store = self.store.read().unwrap();
            let content = store.as_str();

            let block = Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(block_style)
                .title(title.into_centered_line());
            let paragraph =
                Paragraph::new(content).scroll((self.scroller.pos() as u16, 0)).block(block);
            frame.render_widget(paragraph, area);
        }
        self.scroller.render(frame, area);
    }

    fn render_throbber(&mut self, frame: &mut Frame, area: Rect) {
        if !self.loading.load(Ordering::Relaxed) {
            return;
        }
        let symbol = Throbber::default()
            .label("Loading")
            .style(Style::default().fg(Color::White).bg(Color::Green).bold())
            .throbber_style(Style::default().fg(Color::White).bg(Color::Green).bold())
            .throbber_set(BRAILLE_SIX)
            .use_type(WhichUse::Spin);
        frame.render_stateful_widget(
            symbol,
            Rect::new(area.right().saturating_sub(9), area.y, 8, 1),
            &mut self.throbber,
        );
    }

    fn render_actions(&mut self, frame: &mut Frame, area: Rect) {
        let [title_area, buttons_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
        let title = dashed_title_line("actions", area.width - 4).centered();
        frame.render_widget(title, title_area);

        let chunks = Layout::horizontal(ACTION_CONSTRAINTS).spacing(1).split(buttons_area);
        for (idx, label) in ACTIONS.into_iter().enumerate() {
            let active =
                matches!(self.active_pane, ActivePane::Action(active_idx) if active_idx == idx);
            frame.render_widget(Button::new(label).active(active), chunks[idx]);
        }
    }
}

impl Component for CoreConfigComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Config
    }

    fn shortcuts(&self) -> Vec<Shortcut> {
        match self.active_pane {
            ActivePane::Editor => {
                vec![
                    Shortcut::new(vec![
                        Fragment::hl("⇧⇤"),
                        Fragment::raw(" nav "),
                        Fragment::hl("⇥"),
                    ]),
                    Shortcut::new(vec![
                        Fragment::hl(arrow::UP),
                        Fragment::raw(" scroll "),
                        Fragment::hl(arrow::DOWN),
                    ]),
                    Shortcut::new(vec![
                        Fragment::hl("PgUp"),
                        Fragment::raw(" page "),
                        Fragment::hl("PgDn"),
                    ]),
                    Shortcut::from("edit", 0).unwrap(),
                    Shortcut::from("discard", 0).unwrap(),
                    Shortcut::new(vec![Fragment::raw("submit "), Fragment::hl("↵")]),
                ]
            }
            ActivePane::Action(_) => {
                vec![
                    Shortcut::new(vec![
                        Fragment::hl("⇧⇤"),
                        Fragment::raw(" nav "),
                        Fragment::hl("⇥"),
                    ]),
                    Shortcut::new(vec![Fragment::raw("execute "), Fragment::hl("↵")]),
                ]
            }
        }
    }

    fn init(&mut self, api: Arc<Api>) -> Result<()> {
        self.api = Some(api);

        Ok(())
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.action_tx = Some(tx);

        Ok(())
    }

    fn register_config_handler(&mut self, config: Arc<Config>) -> Result<()> {
        self.config = Some(config);
        self.load_core_config()?;

        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if self.handle_pane_switch(key) {
            return Ok(None);
        }

        match self.active_pane {
            ActivePane::Editor => {
                if self.scroller.handle_key_event(key) {
                    return Ok(None);
                }

                match key.code {
                    KeyCode::Char('e') => return self.edit_core_config(),
                    KeyCode::Char('d') => self.load_core_config()?,
                    KeyCode::Enter => {
                        return self.submit_core_config().map(|_| None).or_else(|e| {
                            Ok(Some(Action::Error(("Submit core config", e).into())))
                        });
                    }
                    _ => (),
                }
            }

            ActivePane::Action(idx) => {
                if key.code == KeyCode::Enter {
                    self.handle_action_button(idx)?
                }
            }
        }

        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        if let Action::Tick = action {
            if let Err(err) = self.sync_core_config() {
                self.editor_state = EditorState::SyncFailed;
                error!(error = ?err, "Failed to sync config from external editor");
                return Ok(Some(Action::Error(("Sync config from external editor", err).into())));
            }
            if self.loading.load(Ordering::Relaxed) {
                self.throbber.calc_next();
            }
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        // render border and title
        let title_line = top_title_line("config", Style::default());
        let block = Block::bordered().border_type(BorderType::Rounded).title(title_line);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        // render content
        let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(4)]).split(inner);
        self.render_cfg_preview(frame, chunks[0]);
        self.render_throbber(frame, chunks[0]);
        self.render_actions(frame, chunks[1]);

        Ok(())
    }
}
