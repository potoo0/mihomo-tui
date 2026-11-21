use std::sync::Arc;

use tokio::sync::mpsc::UnboundedSender;

use crate::api::Api;
use crate::app::action::Action;
use crate::config::Config;
use crate::ui::render::RenderRequester;

/// Shared dependencies used to construct fully initialized components.
///
/// Components clone only the handles they need. Constructing a component does
/// not start background work; the component factory calls `Component::start`.
#[derive(Clone)]
pub struct ComponentContext {
    pub config: Arc<Config>,
    pub api: Arc<Api>,
    pub actions: UnboundedSender<Action>,
    pub render: RenderRequester,
}

#[cfg(test)]
impl ComponentContext {
    /// Build dependencies without starting tasks or initializing global stores.
    /// Tests inspecting actions or rendering should replace the corresponding
    /// handle and retain its receiver or scheduler.
    pub fn test_ctx() -> Self {
        let config = Arc::new(Config {
            mihomo_api: "http://127.0.0.1:1".parse().expect("valid test endpoint"),
            ..crate::config::default_config().expect("default test config")
        });
        let api = Arc::new(Api::new(&config).expect("test API client"));
        let (actions, _) = tokio::sync::mpsc::unbounded_channel();
        let scheduler = crate::app::RenderScheduler::default();
        Self { config, api, actions, render: scheduler.requester() }
    }
}
