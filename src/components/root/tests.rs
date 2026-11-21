use std::sync::{Arc, Mutex};

use futures_util::FutureExt;
use strum::IntoEnumIterator;
use tokio::sync::mpsc::{self, unbounded_channel};

use super::*;
use crate::app::RenderScheduler;
use crate::components::proxies::{ProxiesComponent, ProxyDetailComponent};
use crate::components::proxy_providers::{ProxyProviderDetailComponent, ProxyProvidersComponent};
use crate::config::ProxySortConfig;
use crate::models::sort::{ProxySortField, SortDir};
use crate::store::proxy::{Proxies, ProxyDetailSetting, ProxyProviders};

fn test_root() -> (RootComponent, mpsc::UnboundedReceiver<Action>) {
    let (actions, rx) = unbounded_channel();
    let ctx = ComponentContext { actions, ..ComponentContext::test_ctx() };
    let mut root = RootComponent::new(&ctx, ComponentId::Logs);
    // Construct the page without starting network tasks.
    root.components.insert(ComponentId::Logs, Box::new(LogsComponent::new(&ctx)));
    root.components.insert(ComponentId::Filter, Box::new(FilterComponent::new(&ctx)));
    (root, rx)
}

fn dispatch(root: &mut RootComponent, rx: &mut mpsc::UnboundedReceiver<Action>, action: Action) {
    root.update(action).unwrap();
    // Apply queued focus and shortcut actions, just as the application loop does.
    while let Ok(action) = rx.try_recv() {
        root.update(action).unwrap();
    }
}

fn has_render_request(scheduler: &RenderScheduler) -> bool {
    scheduler.wait_for_request().now_or_never().is_some()
}

#[test]
fn popup_factory_matches_requested_id() {
    let ctx = ComponentContext::test_ctx();
    let root = RootComponent::new(&ctx, ComponentId::Logs);
    for id in PopupId::iter() {
        assert_eq!(root.create_popup(id, &ctx).unwrap().popup_id(), id);
    }
}

#[test]
fn closing_proxy_detail_keeps_proxies_store_while_tab_exists() {
    let (mut root, mut rx) = test_root();
    let proxies = ProxiesComponent::new(&root.ctx).unwrap();
    root.components.insert(ComponentId::Proxies, Box::new(proxies));

    let detail_id = ComponentId::Popup(PopupId::ProxyDetail);
    root.components.insert(detail_id, Box::new(ProxyDetailComponent::new(&root.ctx).unwrap()));
    root.popups.push(PopupId::ProxyDetail);

    dispatch(&mut root, &mut rx, PopupId::ProxyDetail.close_action());

    assert!(!root.components.contains_key(&detail_id));
    assert!(root.components.contains_key(&ComponentId::Proxies));
    assert!(root.ctx.stores.get::<Proxies>().is_ok());
}

#[test]
fn destroying_proxies_tab_releases_proxies_store() {
    let (mut root, _rx) = test_root();
    let proxies = ProxiesComponent::new(&root.ctx).unwrap();
    root.components.insert(ComponentId::Proxies, Box::new(proxies));
    assert!(root.ctx.stores.get::<Proxies>().is_ok());

    root.destroy_component(ComponentId::Proxies);

    assert!(!root.components.contains_key(&ComponentId::Proxies));
    assert!(root.ctx.stores.get::<Proxies>().is_err());
}

#[test]
fn destroying_proxies_tab_keeps_detail_sort_setting() {
    let (mut root, _rx) = test_root();
    let proxies = ProxiesComponent::new(&root.ctx).unwrap();
    root.components.insert(ComponentId::Proxies, Box::new(proxies));

    let detail = root.ctx.stores.get::<ProxyDetailSetting>().unwrap();
    detail.update(|setting| {
        setting.sort = Some(ProxySortConfig { field: ProxySortField::Name, dir: SortDir::Desc });
    });

    root.destroy_component(ComponentId::Proxies);

    assert!(root.ctx.stores.get::<Proxies>().is_err());
    let detail = root.ctx.stores.get::<ProxyDetailSetting>().unwrap();
    assert_eq!(
        detail.read(|setting| setting.sort.as_ref().map(|sort| sort.field)),
        Some(ProxySortField::Name)
    );
}

#[test]
fn closing_proxy_provider_detail_keeps_providers_store_while_tab_exists() {
    let (mut root, mut rx) = test_root();
    let providers = ProxyProvidersComponent::new(&root.ctx).unwrap();
    root.components.insert(ComponentId::ProxyProviders, Box::new(providers));

    let detail_id = ComponentId::Popup(PopupId::ProxyProviderDetail);
    root.components
        .insert(detail_id, Box::new(ProxyProviderDetailComponent::new(&root.ctx).unwrap()));
    root.popups.push(PopupId::ProxyProviderDetail);

    dispatch(&mut root, &mut rx, PopupId::ProxyProviderDetail.close_action());

    assert!(!root.components.contains_key(&detail_id));
    assert!(root.components.contains_key(&ComponentId::ProxyProviders));
    assert!(root.ctx.stores.get::<ProxyProviders>().is_ok());
}

#[test]
fn destroying_proxy_providers_tab_releases_providers_store() {
    let (mut root, _rx) = test_root();
    let providers = ProxyProvidersComponent::new(&root.ctx).unwrap();
    root.components.insert(ComponentId::ProxyProviders, Box::new(providers));
    assert!(root.ctx.stores.get::<ProxyProviders>().is_ok());

    root.destroy_component(ComponentId::ProxyProviders);

    assert!(!root.components.contains_key(&ComponentId::ProxyProviders));
    assert!(root.ctx.stores.get::<ProxyProviders>().is_err());
}

#[test]
fn nested_popups_keep_the_parent_and_recreate_after_close() {
    let (mut root, mut rx) = test_root();
    let parent_id = ComponentId::Popup(PopupId::Help);
    let child_id = ComponentId::Popup(PopupId::Language);

    dispatch(&mut root, &mut rx, Action::Help);
    let parent_instance = std::ptr::from_ref(root.components[&parent_id].as_ref());
    dispatch(&mut root, &mut rx, Action::LanguageSelect);
    dispatch(&mut root, &mut rx, Action::LanguageSelect);
    assert_eq!(root.popups, [PopupId::Help, PopupId::Language]);

    dispatch(&mut root, &mut rx, PopupId::Language.close_action());
    assert!(!root.components.contains_key(&child_id));
    assert_eq!(root.focused, Some(FocusTarget::Popup(PopupId::Help)));
    assert!(std::ptr::eq(parent_instance, root.components[&parent_id].as_ref()));

    // Duplicate closure and delayed focus must not recreate the released child.
    dispatch(&mut root, &mut rx, PopupId::Language.close_action());
    dispatch(&mut root, &mut rx, Action::Focus(FocusTarget::Popup(PopupId::Language)));
    assert!(!root.components.contains_key(&child_id));
    assert_eq!(root.popups, [PopupId::Help]);

    dispatch(&mut root, &mut rx, PopupId::Help.close_action());
    assert!(!root.components.contains_key(&parent_id));
    assert!(root.popups.is_empty());
    assert_eq!(root.focused, None);

    dispatch(&mut root, &mut rx, Action::Help);
    assert!(root.components.contains_key(&parent_id));
    assert_eq!(root.popups, [PopupId::Help]);
}

#[test]
fn closing_a_lower_popup_and_dismissing_a_message_keep_the_top_popup() {
    let (mut root, mut rx) = test_root();
    dispatch(&mut root, &mut rx, Action::Help);
    dispatch(&mut root, &mut rx, Action::LanguageSelect);

    dispatch(&mut root, &mut rx, PopupId::Help.close_action());
    assert!(!root.components.contains_key(&ComponentId::Popup(PopupId::Help)));
    assert_eq!(root.popups, [PopupId::Language]);
    assert_eq!(root.focused, Some(FocusTarget::Popup(PopupId::Language)));

    dispatch(&mut root, &mut rx, Action::Error(("titles.error", "test error").into()));
    root.handle_key_event(KeyEvent::from(KeyCode::Esc)).unwrap();
    assert!(root.msg_box.is_none());
    assert!(root.components.contains_key(&ComponentId::Popup(PopupId::Language)));
    assert_eq!(root.popups, [PopupId::Language]);
    assert_eq!(root.focused, Some(FocusTarget::Popup(PopupId::Language)));
}

#[test]
fn only_valid_focus_commands_reach_components() {
    struct FocusObserver(Arc<Mutex<Vec<FocusTarget>>>);

    impl ComponentIdentity for FocusObserver {
        fn id(&self) -> ComponentId {
            ComponentId::Logs
        }
    }

    impl Component for FocusObserver {
        fn update(&mut self, action: Action) -> Result<Option<Action>> {
            match action {
                Action::Focus(target) => self.0.lock().unwrap().push(target),
                Action::Unfocus(_) => panic!("Root must consume Unfocus commands"),
                _ => {}
            }
            Ok(None)
        }

        fn draw(&mut self, _frame: &mut Frame, _area: Rect) -> Result<()> {
            Ok(())
        }
    }

    let (mut root, mut rx) = test_root();
    let observed = Arc::new(Mutex::new(Vec::new()));
    root.components.insert(ComponentId::Logs, Box::new(FocusObserver(Arc::clone(&observed))));
    dispatch(&mut root, &mut rx, Action::Help);
    dispatch(&mut root, &mut rx, Action::LanguageSelect);
    dispatch(&mut root, &mut rx, Action::Focus(FocusTarget::Popup(PopupId::Help)));
    dispatch(&mut root, &mut rx, PopupId::Language.close_action());
    dispatch(&mut root, &mut rx, Action::Focus(FocusTarget::Popup(PopupId::Language)));
    dispatch(&mut root, &mut rx, Action::Focus(FocusTarget::Filter));
    dispatch(&mut root, &mut rx, Action::Unfocus(FocusTarget::Filter));
    assert_eq!(root.focused, Some(FocusTarget::Popup(PopupId::Help)));
    assert_eq!(observed.lock().unwrap().len(), 3);

    let close = root.handle_key_event(KeyEvent::from(KeyCode::Esc)).unwrap().unwrap();
    assert!(matches!(close, Action::Unfocus(FocusTarget::Popup(PopupId::Help))));
    dispatch(&mut root, &mut rx, close);
    assert!(root.popups.is_empty());
    assert!(!root.components.contains_key(&ComponentId::Popup(PopupId::Help)));

    dispatch(&mut root, &mut rx, Action::Focus(FocusTarget::Filter));
    assert_eq!(root.focused, Some(FocusTarget::Filter));
    dispatch(&mut root, &mut rx, Action::Unfocus(FocusTarget::Filter));

    assert!(root.components.contains_key(&ComponentId::Filter));
    assert_eq!(root.focused, None);
    assert_eq!(
        *observed.lock().unwrap(),
        [
            FocusTarget::Popup(PopupId::Help),
            FocusTarget::Popup(PopupId::Language),
            FocusTarget::Popup(PopupId::Help),
            FocusTarget::Filter,
        ]
    );
}

#[test]
fn constructing_stream_tabs_does_not_require_a_runtime_or_start_work() {
    let scheduler = RenderScheduler::default();
    let (actions, mut rx) = unbounded_channel();
    let ctx =
        ComponentContext { actions, render: scheduler.requester(), ..ComponentContext::test_ctx() };
    let streams = ConnectionStreamHub::new();
    // This synchronous test deliberately has no Tokio runtime. Only start()
    // may spawn tasks; construction must leave streams and rendering idle.
    let _overview = OverviewComponent::new(&ctx, streams.stats_rx());
    let _connections = ConnectionsComponent::new(&ctx, streams.conns_rx()).unwrap();
    let _logs = LogsComponent::new(&ctx);
    let root = RootComponent::new(&ctx, ComponentId::Logs);

    assert!(!streams.is_active());
    assert!(!root.connections.is_active());
    assert_eq!(root.components.len(), 2);
    assert!(!has_render_request(&scheduler));
    assert!(matches!(rx.try_recv(), Err(mpsc::error::TryRecvError::Empty)));
}

#[test]
fn language_shortcut_respects_focused_input_and_modifiers() {
    let mut root = RootComponent::new(&ComponentContext::test_ctx(), ComponentId::Proxies);
    assert!(matches!(
        root.handle_key_event(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::SHIFT)).unwrap(),
        Some(Action::LanguageSelect)
    ));

    root.components.insert(ComponentId::Filter, Box::new(FilterComponent::new(&root.ctx)));
    root.focused = Some(FocusTarget::Filter);
    assert!(
        root.handle_key_event(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::NONE))
            .unwrap()
            .is_none()
    );
    assert!(
        root.handle_key_event(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::ALT))
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        root.handle_key_event(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL)).unwrap(),
        Some(Action::Tick)
    ));
}

#[tokio::test]
async fn version_refresh_still_runs_after_updates_popup_is_released() {
    let scheduler = RenderScheduler::default();
    let (actions, _rx) = unbounded_channel();
    let ctx =
        ComponentContext { actions, render: scheduler.requester(), ..ComponentContext::test_ctx() };
    let mut root = RootComponent::new(&ctx, ComponentId::Logs);
    root.components.insert(ComponentId::Logs, Box::new(LogsComponent::new(&ctx)));
    root.update(Action::AppUpdateRequest).unwrap();
    root.update(Action::Unfocus(FocusTarget::Popup(PopupId::Updates))).unwrap();
    assert!(!root.components.contains_key(&ComponentId::Popup(PopupId::Updates)));

    // An in-progress refresh is a deterministic no-network path. Header
    // must still handle the action and notify rendering when it completes.
    root.update_state.lock().app = crate::version_update::VersionStatus::Refreshing;
    root.update(Action::RefreshVersion).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), scheduler.wait_for_request())
        .await
        .unwrap();
    assert!(!root.components.contains_key(&ComponentId::Popup(PopupId::Updates)));
}
