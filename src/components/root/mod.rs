mod connection_stream;
mod render_gates;

use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use tracing::{debug, info};

use self::connection_stream::ConnectionStreamHub;
use self::render_gates::TabRenderRequesters;
use crate::app::action::Action;
use crate::components::connections::{
    ConnectionBatchTerminateComponent, ConnectionDetailComponent, ConnectionTerminateComponent,
    ConnectionsComponent, ConnectionsSettingComponent,
};
use crate::components::core_config::CoreConfigComponent;
use crate::components::dns_query::DnsQueryComponent;
use crate::components::filter::FilterComponent;
use crate::components::footer::FooterComponent;
use crate::components::header::HeaderComponent;
use crate::components::help::HelpComponent;
use crate::components::language::LanguageComponent;
use crate::components::logs::LogsComponent;
use crate::components::msg_box::MsgBoxComponent;
use crate::components::overview::OverviewComponent;
use crate::components::proxies::{
    ProxiesComponent, ProxyDetailComponent, ProxyInspectComponent, ProxySettingComponent,
};
use crate::components::proxy_providers::{ProxyProviderDetailComponent, ProxyProvidersComponent};
use crate::components::rule_providers::RuleProvidersComponent;
use crate::components::rules::RulesComponent;
use crate::components::updates::UpdatesComponent;
use crate::components::{
    Component, ComponentContext, ComponentId, ComponentIdentity, FocusTarget, PopupComponent,
    PopupId, TABS,
};
use crate::shortcut;
use crate::ui::layout::top_title_line;
use crate::ui::render::RenderRequester;
use crate::ui::shortcut::{Fragment, Shortcut};
use crate::version_update::SharedVersionUpdateState;

/// Minimum terminal area `(width, height)` to render the UI properly.
const MIN_AREA: (u16, u16) = (80, 18);
/// 120 seconds at 4 ticks per second
const IDLE_TICKS: u16 = 120 * 4;

pub struct RootComponent {
    ctx: ComponentContext,

    render_requesters: TabRenderRequesters,
    connections: ConnectionStreamHub,
    update_state: SharedVersionUpdateState,

    current_tab: ComponentId,
    idle_tabs: HashMap<ComponentId, u16>,
    components: HashMap<ComponentId, Box<dyn Component>>,

    /// UI priority (input & render): `msg_box` > `focused` > `popups` > normal.
    /// Message box lifecycle is owned and eagerly cleared by RootComponent
    msg_box: Option<MsgBoxComponent>,
    focused: Option<FocusTarget>,
    popups: Vec<PopupId>,
}

impl RootComponent {
    pub fn new(ctx: &ComponentContext, initial_tab: ComponentId) -> Self {
        let update_state = SharedVersionUpdateState::default();
        let components: Vec<Box<dyn Component>> = vec![
            Box::new(HeaderComponent::new(ctx, update_state.clone())),
            Box::new(FooterComponent::new(ctx)),
        ];
        let components = components.into_iter().map(|c| (c.id(), c)).collect::<HashMap<_, _>>();
        Self {
            ctx: ctx.clone(),

            current_tab: initial_tab,
            popups: Default::default(),
            focused: Default::default(),
            idle_tabs: Default::default(),
            msg_box: Default::default(),
            components,
            render_requesters: TabRenderRequesters::new(ctx.render.clone()),
            connections: ConnectionStreamHub::new(),
            update_state,
        }
    }

    fn create_component(
        &self,
        id: ComponentId,
        render_requester: RenderRequester,
    ) -> Result<Box<dyn Component>> {
        let ctx = ComponentContext { render: render_requester, ..self.ctx.clone() };

        let mut component: Box<dyn Component> = match id {
            ComponentId::Overview => {
                Box::new(OverviewComponent::new(&ctx, self.connections.stats_rx()))
            }
            ComponentId::Connections => {
                Box::new(ConnectionsComponent::new(&ctx, self.connections.conns_rx())?)
            }
            ComponentId::Proxies => Box::new(ProxiesComponent::new(&ctx)?),
            ComponentId::ProxyProviders => Box::new(ProxyProvidersComponent::new(&ctx)?),
            ComponentId::Logs => Box::new(LogsComponent::new(&ctx)),
            ComponentId::Rules => Box::new(RulesComponent::new(&ctx)),
            ComponentId::RuleProviders => Box::new(RuleProvidersComponent::new(&ctx)),
            ComponentId::Config => Box::new(CoreConfigComponent::new(&ctx)),
            ComponentId::Filter => Box::new(FilterComponent::new(&ctx)),
            ComponentId::Popup(id) => self.create_popup(id, &ctx)?,
            _ => bail!("unsupported component `{:?}`", id),
        };

        debug!("Initializing component `{:?}`", id);
        component.start().with_context(|| format!("failed to start {id:?}"))?;

        Ok(component)
    }

    fn create_popup(&self, id: PopupId, ctx: &ComponentContext) -> Result<Box<dyn PopupComponent>> {
        Ok(match id {
            PopupId::Help => Box::new(HelpComponent::new(ctx)),
            PopupId::Updates => Box::new(UpdatesComponent::new(ctx, self.update_state.clone())),
            PopupId::Language => Box::new(LanguageComponent::new(ctx)),
            PopupId::ConnectionDetail => Box::new(ConnectionDetailComponent::new(ctx)),
            PopupId::ConnectionTerminate => Box::new(ConnectionTerminateComponent::new(ctx)),
            PopupId::ConnectionBatchTerminate => {
                Box::new(ConnectionBatchTerminateComponent::new(ctx))
            }
            PopupId::ConnectionsSetting => Box::new(ConnectionsSettingComponent::new(ctx)?),
            PopupId::ProxyDetail => Box::new(ProxyDetailComponent::new(ctx)?),
            PopupId::ProxyInspect => Box::new(ProxyInspectComponent::new(ctx)),
            PopupId::ProxySetting => Box::new(ProxySettingComponent::new(ctx)?),
            PopupId::ProxyProviderDetail => Box::new(ProxyProviderDetailComponent::new(ctx)?),
            PopupId::DnsQuery => Box::new(DnsQueryComponent::new(ctx)),
        })
    }

    fn get_or_start(&mut self, id: impl Into<ComponentId>) -> Result<&mut Box<dyn Component>> {
        let id = id.into();
        // TODO: Once Polonius is stable, use `get_mut` as an early-return fast path to avoid
        // the second lookup for initialized components (NLL problem case #3).
        if !self.components.contains_key(&id) {
            let render_requester = self.render_requesters.requester_for(id, self.current_tab);
            let component = self.create_component(id, render_requester)?;
            self.components.insert(id, component);
        }

        Ok(self.components.get_mut(&id).expect("component must exist after initialization"))
    }

    fn open_popup(&mut self, id: PopupId) -> Result<()> {
        info!(popup = ?id, depth = self.popups.len(), "Opening popup");

        // get and init component, send shortcuts of current tab to footer
        let shortcuts = self.get_or_start(id)?.shortcuts();
        self.popups.retain(|popup| *popup != id);
        self.popups.push(id);
        let tx = &self.ctx.actions;
        tx.send(Action::Shortcuts(shortcuts))?;

        // focus the popup component
        tx.send(Action::Focus(FocusTarget::Popup(id)))?;

        Ok(())
    }

    fn close_popup(&mut self, id: PopupId) -> Result<()> {
        if !self.popups.contains(&id) {
            return Ok(());
        }

        let should_restore =
            self.popups.last() == Some(&id) || self.focused == Some(FocusTarget::Popup(id));
        self.popups.retain(|popup| *popup != id);
        drop(self.components.remove(&ComponentId::Popup(id)));
        self.render_requesters.remove(ComponentId::Popup(id));
        self.ctx.stores.release_unused(|id| self.components.contains_key(&id));

        if !should_restore {
            return Ok(());
        }

        let next_focus = self.popups.last().copied();
        self.focused = next_focus.map(FocusTarget::Popup);
        let shortcuts = if let Some(next) = next_focus {
            self.components
                .get(&ComponentId::Popup(next))
                .context("popup stack must contain existing components")?
                .shortcuts()
        } else {
            self.get_or_start(self.current_tab)?.shortcuts()
        };
        let tx = &self.ctx.actions;
        tx.send(Action::Shortcuts(shortcuts))?;
        if let Some(next_focus) = next_focus {
            tx.send(Action::Focus(FocusTarget::Popup(next_focus)))?;
        }
        Ok(())
    }

    fn set_shortcuts(&mut self, shortcuts: Vec<Shortcut>) -> Result<()> {
        let mut visible = Vec::with_capacity(shortcuts.len() + 2);
        if self.popups.is_empty() {
            visible.push(Shortcut::new(vec![Fragment::key("?")]));
            visible.push(shortcut!("quit", 0));
        }
        visible.extend(shortcuts);
        self.get_or_start(ComponentId::Footer)?.update(Action::Shortcuts(visible))?;
        Ok(())
    }

    fn start_conn_if_needed(&mut self) -> Result<()> {
        self.connections.start_if_needed(&self.ctx.api, self.current_tab)
    }

    fn renew_idle(&mut self, to: ComponentId) {
        self.idle_tabs.remove(&to);
        if self.current_tab != to {
            self.idle_tabs.insert(self.current_tab, IDLE_TICKS);
        }
    }

    fn destroy_component(&mut self, id: ComponentId) {
        // double check
        if id == self.current_tab {
            return;
        }
        if let Some(mut component) = self.components.remove(&id) {
            self.idle_tabs.remove(&id);
            self.render_requesters.remove(id);
            // Quit doubles as the component shutdown signal:
            // cancelling tokens and flushing pending work is correct whether the app
            // exits or the idle tab is recycled.
            if let Ok(Some(action)) = component.update(Action::Quit) {
                let _ = self.ctx.actions.send(action);
            }
            info!("Destroyed idle component {:?}", id);
        }
        self.ctx.stores.release_unused(|id| self.components.contains_key(&id));
    }

    fn on_tick(&mut self) {
        // decrement idle counters
        let mut to_remove = vec![];
        for (&id, ticks) in self.idle_tabs.iter_mut() {
            *ticks = ticks.saturating_sub(1);
            if *ticks == 0 {
                to_remove.push(id);
            }
        }
        for id in to_remove {
            self.destroy_component(id);
        }
        // stop connections if no tab needs it
        let has_idle_consumer =
            self.idle_tabs.keys().any(|id| ConnectionStreamHub::needs_stream(*id));
        self.connections.stop_if_unused(self.current_tab, has_idle_consumer);
    }

    fn handle_global_shortcut(&mut self, key: KeyEvent) -> Option<Action> {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => return Some(Action::Quit),
                KeyCode::Char('l') => {
                    info!("Clearing idle tabs by Ctrl+L shortcut");
                    for id in self.idle_tabs.keys().cloned().collect::<Vec<_>>() {
                        self.destroy_component(id);
                    }
                    return Some(Action::Tick);
                }
                KeyCode::Char('u')
                    if key.modifiers == KeyModifiers::CONTROL
                        && self.popups.is_empty()
                        && self.focused.is_none()
                        && self.msg_box.is_none() =>
                {
                    return Some(Action::AppUpdateRequest);
                }
                _ => {}
            }
        }
        None
    }
}

impl ComponentIdentity for RootComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Root
    }
}

impl Component for RootComponent {
    fn start(&mut self) -> Result<()> {
        for component in self.components.values_mut() {
            component.start()?;
        }
        self.start_conn_if_needed()
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        // handle global shortcuts
        if let Some(action) = self.handle_global_shortcut(key) {
            return Ok(Some(action));
        }

        // The message box component
        if let Some(msg_box) = &self.msg_box {
            if msg_box.should_close_on_key(key) {
                self.msg_box = None;
            }
            return Ok(None);
        }

        // The focused component exclusively handles key events.
        if let Some(focused) = self.focused {
            return self.get_or_start(focused)?.handle_key_event(key);
        }

        match key.code {
            KeyCode::Char('q') => return Ok(Some(Action::Quit)),
            KeyCode::Char('?') => return Ok(Some(Action::Help)),
            KeyCode::Char('L')
                if matches!(key.modifiers, KeyModifiers::NONE | KeyModifiers::SHIFT) =>
            {
                return Ok(Some(Action::LanguageSelect));
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let index = (c as u8 - b'0') as usize;
                if let Some(component_id) = TABS.get(index.saturating_sub(1)) {
                    self.ctx.actions.send(Action::TabSwitch(*component_id))?;
                }
                return Ok(None);
            }
            _ => {}
        }
        debug!("Try handling key event: tab={:?}, key={:?}", self.current_tab, key);
        self.get_or_start(self.current_tab)?.handle_key_event(key)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        let action_tx = self.ctx.actions.clone();
        match action {
            Action::Shortcuts(shortcuts) => {
                self.set_shortcuts(shortcuts)?;
                return Ok(None);
            }
            Action::Quit => self.connections.stop(),
            Action::Tick => self.on_tick(),
            Action::Error(err) => {
                self.msg_box = Some(MsgBoxComponent::error(err));
                return Ok(None);
            }
            Action::Info(info) => {
                self.msg_box = Some(MsgBoxComponent::info(info));
                return Ok(None);
            }
            Action::TabSwitch(to) => {
                let from = self.current_tab;
                self.renew_idle(to);
                self.current_tab = to;
                self.render_requesters.switch_tab(from, to);
                self.start_conn_if_needed()?;
                // get and init component, send shortcuts of current tab to footer
                let shortcuts = self.get_or_start(self.current_tab)?.shortcuts();
                if self.current_tab.supports_filter() {
                    self.get_or_start(ComponentId::Filter)?;
                }
                action_tx.send(Action::Shortcuts(shortcuts))?;
            }
            Action::AppUpdateRequest => self.open_popup(PopupId::Updates)?,
            Action::Help => self.open_popup(PopupId::Help)?,
            Action::LanguageSelect => self.open_popup(PopupId::Language)?,
            Action::ConnectionDetail(_) => self.open_popup(PopupId::ConnectionDetail)?,
            Action::ConnectionsSetting(_) => self.open_popup(PopupId::ConnectionsSetting)?,
            Action::ProxyDetail(_) => self.open_popup(PopupId::ProxyDetail)?,
            Action::ProxyInspect(_) => self.open_popup(PopupId::ProxyInspect)?,
            Action::ProxySetting => self.open_popup(PopupId::ProxySetting)?,
            Action::ProxyProviderDetail(_) => self.open_popup(PopupId::ProxyProviderDetail)?,
            Action::ConnectionTerminateRequest(_) => {
                self.open_popup(PopupId::ConnectionTerminate)?
            }
            Action::ConnectionBatchTerminateRequest(_) => {
                self.open_popup(PopupId::ConnectionBatchTerminate)?
            }
            Action::DnsQuery => self.open_popup(PopupId::DnsQuery)?,
            Action::Focus(target) => {
                // Queued focus requests must respect the current popup stack.
                let allowed = match target {
                    FocusTarget::Filter => self.popups.is_empty(),
                    FocusTarget::Popup(id) => self.popups.last() == Some(&id),
                };
                if !allowed {
                    return Ok(None);
                }
                self.focused = Some(target);
            }
            Action::Unfocus(target) => {
                match target {
                    FocusTarget::Filter if self.focused == Some(FocusTarget::Filter) => {
                        self.focused = None;
                    }
                    FocusTarget::Filter => (),
                    FocusTarget::Popup(id) => self.close_popup(id)?,
                }
                return Ok(None);
            }
            _ => {}
        }

        // Propagate actions.
        let wake_idle = action.broadcasts_to_idle();
        for (component_id, component) in self.components.iter_mut() {
            if self.idle_tabs.contains_key(component_id) && !wake_idle {
                continue;
            }

            if let Some(action) = component.update(action.clone())? {
                action_tx.send(action)?;
            }
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        if area.width < MIN_AREA.0 || area.height < MIN_AREA.1 {
            let lines = vec![
                Line::from(rust_i18n::t!("ui.terminal_too_small")).centered(),
                area_msg_line(area.width, area.height).centered(),
                Line::raw(""),
                Line::from(rust_i18n::t!("ui.expected_size")).centered(),
                area_msg_line(MIN_AREA.0, MIN_AREA.1).centered(),
            ];
            let block = Block::default()
                .border_type(BorderType::Rounded)
                .title(top_title_line("titles.error", Color::Red))
                .borders(Borders::ALL);
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
            return Ok(());
        }
        let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);

        // draw header
        self.get_or_start(ComponentId::Header)?.draw(frame, chunks[0])?;

        // draw main area
        if self.current_tab.supports_filter() {
            let inner_chunks =
                Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(chunks[1]);
            self.get_or_start(ComponentId::Filter)?.draw(frame, inner_chunks[0])?;
            self.get_or_start(self.current_tab)?.draw(frame, inner_chunks[1])?;
        } else {
            self.get_or_start(self.current_tab)?.draw(frame, chunks[1])?;
        }

        // Draw popup layers from bottom to top.
        for popup in self.popups.clone() {
            self.get_or_start(popup)?.draw(frame, chunks[1])?;
        }
        self.msg_box.as_ref().map(|c| c.draw(frame, area)).transpose()?;

        // draw footer
        // get last row of main area for footer, with margin left/right = 1
        let footer_area = Rect::new(area.x + 1, area.y + area.height - 1, area.width - 2, 1);
        self.get_or_start(ComponentId::Footer)?.draw(frame, footer_area)?;
        Ok(())
    }
}

fn area_msg_line<'a>(width: u16, height: u16) -> Line<'a> {
    Line::default().spans(vec![
        "Width = ".bold(),
        Span::raw(width.to_string()).cyan(),
        " Height = ".bold(),
        Span::raw(height.to_string()).cyan(),
    ])
}

#[cfg(test)]
mod tests;
