use futures_util::FutureExt;
use tokio::sync::mpsc::{self, unbounded_channel};

use super::*;
use crate::app::RenderScheduler;

struct TestPopup(ComponentId);

impl Component for TestPopup {
    fn id(&self) -> ComponentId {
        self.0
    }

    fn draw(&mut self, _frame: &mut Frame, _area: Rect) -> Result<()> {
        Ok(())
    }
}

fn popup_test_root() -> (RootComponent, tokio::sync::mpsc::UnboundedReceiver<Action>) {
    let (tx, rx) = unbounded_channel();
    let ctx = ComponentContext { actions: tx, ..ComponentContext::test_ctx() };
    let mut root = RootComponent::new(&ctx, ComponentId::Proxies);
    for id in [
        ComponentId::Proxies,
        ComponentId::Help,
        ComponentId::Language,
        ComponentId::ProxyDetail,
        ComponentId::ProxyInspect,
    ] {
        root.components.insert(id, Box::new(TestPopup(id)));
    }
    (root, rx)
}
fn has_render_request(scheduler: &RenderScheduler) -> bool {
    scheduler.wait_for_request().now_or_never().is_some()
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
    let _connections = ConnectionsComponent::new(&ctx, streams.conns_rx());
    let _logs = LogsComponent::new(&ctx);
    let root = RootComponent::new(&ctx, ComponentId::Logs);

    assert!(!streams.is_active());
    assert!(!root.connections.is_active());
    assert_eq!(root.components.len(), 2);
    assert!(!has_render_request(&scheduler));
    assert!(matches!(rx.try_recv(), Err(mpsc::error::TryRecvError::Empty)));
}

#[test]
fn lazy_component_is_reused_and_can_be_recreated_after_retirement() {
    let mut root = RootComponent::new(&ComponentContext::test_ctx(), ComponentId::Logs);
    assert!(!root.components.contains_key(&ComponentId::Help));

    root.get_or_start(ComponentId::Help).unwrap();
    let first = root.components[&ComponentId::Help].as_ref() as *const dyn Component;
    let second = root.get_or_start(ComponentId::Help).unwrap().as_ref() as *const dyn Component;
    assert!(std::ptr::eq(first, second));

    root.destroy_component(ComponentId::Help);
    assert!(!root.components.contains_key(&ComponentId::Help));
    assert_eq!(root.get_or_start(ComponentId::Help).unwrap().id(), ComponentId::Help);
}

#[test]
fn language_shortcut_respects_focused_input_and_modifiers() {
    let mut root = RootComponent::new(&ComponentContext::test_ctx(), ComponentId::Proxies);
    assert!(matches!(
        root.handle_key_event(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::SHIFT)).unwrap(),
        Some(Action::LanguageSelect)
    ));

    root.components.insert(ComponentId::Filter, Box::new(FilterComponent::new(&root.ctx)));
    root.focused = Some(ComponentId::Filter);
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

#[test]
fn popup_open_stack_cases() {
    struct Case {
        name: &'static str,
        initial: &'static [ComponentId],
        opened: &'static [ComponentId],
        expected: &'static [ComponentId],
    }

    let cases = [
        Case {
            name: "opening a popup on an empty stack",
            initial: &[],
            opened: &[ComponentId::Help],
            expected: &[ComponentId::Help],
        },
        Case {
            name: "opening the active popup again",
            initial: &[],
            opened: &[ComponentId::Help, ComponentId::Help],
            expected: &[ComponentId::Help],
        },
        Case {
            name: "opening a child popup",
            initial: &[ComponentId::Help],
            opened: &[ComponentId::Language],
            expected: &[ComponentId::Help, ComponentId::Language],
        },
        Case {
            name: "reopening a popup below the active popup",
            initial: &[ComponentId::Help, ComponentId::Language],
            opened: &[ComponentId::Help],
            expected: &[ComponentId::Language, ComponentId::Help],
        },
    ];

    for case in cases {
        let (mut root, _rx) = popup_test_root();
        root.popups = case.initial.to_vec();
        for id in case.opened {
            root.open_popup(*id).unwrap();
        }
        assert_eq!(root.popups, case.expected, "{}", case.name);
    }
}

#[test]
fn popup_unfocus_stack_cases() {
    struct Case {
        name: &'static str,
        initial: &'static [ComponentId],
        focused: ComponentId,
        unfocused: ComponentId,
        expected: &'static [ComponentId],
        expected_focused: Option<ComponentId>,
        expect_shortcuts: bool,
        expected_focus: Option<ComponentId>,
    }

    let cases = [
        Case {
            name: "closing a child popup",
            initial: &[ComponentId::Help, ComponentId::Language],
            focused: ComponentId::Language,
            unfocused: ComponentId::Language,
            expected: &[ComponentId::Help],
            expected_focused: Some(ComponentId::Help),
            expect_shortcuts: true,
            expected_focus: Some(ComponentId::Help),
        },
        Case {
            name: "closing the last popup",
            initial: &[ComponentId::Help],
            focused: ComponentId::Help,
            unfocused: ComponentId::Help,
            expected: &[],
            expected_focused: None,
            expect_shortcuts: true,
            expected_focus: None,
        },
        Case {
            name: "unfocusing a non-popup component",
            initial: &[ComponentId::Help],
            focused: ComponentId::Filter,
            unfocused: ComponentId::Filter,
            expected: &[ComponentId::Help],
            expected_focused: None,
            expect_shortcuts: false,
            expected_focus: None,
        },
        Case {
            name: "closing an inactive popup below the focused popup",
            initial: &[ComponentId::ProxyDetail, ComponentId::Language],
            focused: ComponentId::Language,
            unfocused: ComponentId::ProxyDetail,
            expected: &[ComponentId::Language],
            expected_focused: Some(ComponentId::Language),
            expect_shortcuts: false,
            expected_focus: None,
        },
        Case {
            name: "closing proxy inspect",
            initial: &[ComponentId::ProxyDetail, ComponentId::ProxyInspect],
            focused: ComponentId::ProxyInspect,
            unfocused: ComponentId::ProxyInspect,
            expected: &[ComponentId::ProxyDetail],
            expected_focused: Some(ComponentId::ProxyDetail),
            expect_shortcuts: true,
            expected_focus: Some(ComponentId::ProxyDetail),
        },
    ];

    for case in cases {
        let (mut root, mut rx) = popup_test_root();
        root.popups = case.initial.to_vec();
        root.focused = Some(case.focused);

        root.update(Action::Unfocus(case.unfocused)).unwrap();

        assert_eq!(root.popups, case.expected, "{}: popup stack", case.name);
        assert_eq!(root.focused, case.expected_focused, "{}: focused state", case.name);
        if case.expect_shortcuts {
            assert!(
                matches!(rx.try_recv(), Ok(Action::Shortcuts(_))),
                "{}: expected shortcut refresh",
                case.name
            );
        } else {
            assert!(rx.try_recv().is_err(), "{}: unexpected shortcut refresh", case.name);
        }
        match case.expected_focus {
            Some(focus) => {
                assert!(
                    matches!(rx.try_recv(), Ok(Action::Focus(id)) if id == focus),
                    "{}: expected focus {focus:?}",
                    case.name
                );
            }
            None => {
                assert!(rx.try_recv().is_err(), "{}: unexpected focus action", case.name);
            }
        }
    }
}
