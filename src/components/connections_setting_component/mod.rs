mod columns;
mod source_ip_alias;

use anyhow::Result;
use columns::ColumnsSettingPane;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::prelude::{Color, Line, Span, Style};
use ratatui::symbols::line::{BOTTOM_LEFT, BOTTOM_RIGHT};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use source_ip_alias::SourceIpAliasSettingPane;
use tokio::sync::mpsc::UnboundedSender;

use crate::action::Action;
use crate::components::{Component, ComponentId};
use crate::models::sort::SortSpec;
use crate::store::connections::with_alive_column;
use crate::store::connections_setting::ConnectionsSetting;
use crate::utils::input::KeyOutcome;
use crate::utils::text_ui::{popup_area, top_title_line};
use crate::widgets::shortcut::{Fragment, Shortcut, ShortcutMode, shortcuts_full_width};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ActivePane {
    #[default]
    Columns,
    SourceIpAlias,
}

impl ActivePane {
    fn next(self) -> Self {
        match self {
            Self::Columns => Self::SourceIpAlias,
            Self::SourceIpAlias => Self::Columns,
        }
    }

    fn prev(self) -> Self {
        self.next()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Direction {
    Prev,
    Next,
}

pub(super) trait SettingPane {
    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![]
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> KeyOutcome;

    fn draw_content(&mut self, frame: &mut Frame, area: Rect, active: bool);

    fn draw(&mut self, frame: &mut Frame, area: Rect, active: bool) {
        self.draw_content(frame, area, active);
        let shortcuts = self.shortcuts();
        if shortcuts.is_empty() {
            return;
        }

        let footer_area = Rect::new(area.x + 1, area.y + area.height - 1, area.width - 2, 1);
        let full_width = shortcuts_full_width(&shortcuts, 2);
        let mode = if full_width <= footer_area.width as usize {
            ShortcutMode::Full
        } else {
            ShortcutMode::Compact
        };
        let mut spans = Vec::with_capacity(shortcuts.len());
        for shortcut in &shortcuts {
            spans.push(Span::raw(BOTTOM_RIGHT));
            spans.extend(shortcut.spans_for(mode, None));
            spans.push(Span::raw(BOTTOM_LEFT));
        }

        frame.render_widget(Line::from(spans), footer_area);
    }

    fn error(&self) -> Option<&str> {
        None
    }

    fn clear_error(&mut self) {}
}

#[derive(Debug, Default)]
pub struct ConnectionsSettingComponent {
    show: bool,
    active_pane: ActivePane,
    columns: ColumnsSettingPane,
    source_ip_alias: SourceIpAliasSettingPane,
    action_tx: Option<UnboundedSender<Action>>,
}

impl ConnectionsSettingComponent {
    fn show(&mut self, source_ips: Vec<String>) {
        let setting = ConnectionsSetting::snapshot();
        self.show = true;
        self.active_pane = ActivePane::Columns;
        self.columns.load(&setting.columns);
        self.source_ip_alias.load(source_ips, &setting.source_ip_alias);
    }

    fn hide(&mut self) {
        self.show = false;
        self.columns.reset();
        self.source_ip_alias.reset();
    }

    fn switch_pane(&mut self, next: ActivePane) {
        if self.active_pane == next {
            return;
        }

        self.active_pane = next;
        self.clear_active_error();
        if let Some(tx) = &self.action_tx {
            let _ = tx.send(Action::Shortcuts(self.shortcuts()));
        }
    }

    fn active_setting_pane_mut(&mut self) -> &mut dyn SettingPane {
        match self.active_pane {
            ActivePane::Columns => &mut self.columns,
            ActivePane::SourceIpAlias => &mut self.source_ip_alias,
        }
    }

    fn active_error(&self) -> Option<&str> {
        match self.active_pane {
            ActivePane::Columns => self.columns.error(),
            ActivePane::SourceIpAlias => self.source_ip_alias.error(),
        }
    }

    fn clear_active_error(&mut self) {
        self.active_setting_pane_mut().clear_error();
    }

    fn save(&mut self) -> Result<Option<Action>> {
        let columns = self.columns.selected_column_indices();
        if columns.is_empty() {
            self.columns.set_error("At least one column must be selected");
            return Ok(None);
        }
        let columns = with_alive_column(columns);

        let source_ip_alias = self.source_ip_alias.aliases();
        ConnectionsSetting::update(|setting| {
            // update source ip alias
            setting.source_ip_alias = source_ip_alias;

            // update column and sort
            let prev_sort = setting
                .query_state
                .sort
                .and_then(|sort| setting.columns.get(sort.col).map(|&col| (col, sort.dir)));
            setting.query_state.sort = prev_sort.and_then(|(prev_sort_col, dir)| {
                // try to restore the previous sort column as an index in the new visible columns
                columns
                    .iter()
                    .position(|&col| col == prev_sort_col)
                    .map(|col| SortSpec { col, dir })
            });
            setting.query_state.set_max_cols(columns.len());
            setting.columns = columns;
        });

        self.hide();
        if let Some(tx) = &self.action_tx {
            tx.send(Action::ConnectionsSettingChanged)?;
        }
        Ok(Some(Action::Unfocus))
    }

    fn render_settings(&mut self, frame: &mut Frame, area: Rect) {
        let [columns_area, alias_area, _, status_area] = Layout::vertical([
            Constraint::Length(5), // `Columns` pane
            Constraint::Min(8),    // `Source IP Alias` pane
            Constraint::Length(1), // gap
            Constraint::Length(1), // status
        ])
        .areas(area);

        self.columns.draw(frame, columns_area, self.active_pane == ActivePane::Columns);
        self.source_ip_alias.draw(frame, alias_area, self.active_pane == ActivePane::SourceIpAlias);
        self.render_status(frame, status_area);
    }

    fn render_status(&self, frame: &mut Frame, area: Rect) {
        if let Some(error) = self.active_error() {
            let msg = Line::from(Span::styled(error, Style::default().fg(Color::Red)));
            frame.render_widget(Paragraph::new(msg), area);
        }
    }
}

impl Component for ConnectionsSettingComponent {
    fn id(&self) -> ComponentId {
        ComponentId::ConnectionsSetting
    }

    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new(vec![Fragment::hl("⇧⇤"), Fragment::raw(" pane nav "), Fragment::hl("⇥")])
                .compact(vec![
                    Fragment::hl("⇧⇤"),
                    Fragment::raw("/"),
                    Fragment::hl("⇥"),
                    Fragment::raw(" pane"),
                ]),
            Shortcut::new(vec![Fragment::raw("apply "), Fragment::hl("↵")]),
            Shortcut::new(vec![Fragment::raw("cancel "), Fragment::hl("Esc")]),
        ]
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if self.active_setting_pane_mut().handle_key_event(key).is_consumed() {
            return Ok(None);
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.hide();
                return Ok(Some(Action::Unfocus));
            }
            KeyCode::Tab => self.switch_pane(self.active_pane.next()),
            KeyCode::BackTab => self.switch_pane(self.active_pane.prev()),
            KeyCode::Enter => return self.save(),
            _ => {}
        }

        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        if let Action::ConnectionsSetting(source_ips) = action {
            self.show(source_ips);
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        if !self.show {
            return Ok(());
        }

        let area = popup_area(area, 80, 80);
        frame.render_widget(Clear, area);
        let area = area.inner(Margin::new(2, 1));

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Color::LightBlue)
            .title(top_title_line("connections settings", Style::default()));
        let content_area = block.inner(area);
        frame.render_widget(block, area);
        self.render_settings(frame, content_area);

        Ok(())
    }
}
