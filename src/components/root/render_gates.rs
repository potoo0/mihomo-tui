use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::components::ComponentId;
use crate::ui::render::RenderRequester;

pub(super) struct TabRenderRequesters {
    requester: RenderRequester,
    /// Render notification gates for tabs with continuously running streams.
    gates: HashMap<ComponentId, Arc<AtomicBool>>,
}

impl TabRenderRequesters {
    pub(super) fn new(requester: RenderRequester) -> Self {
        Self { requester, gates: HashMap::new() }
    }

    pub(super) fn requester_for(
        &mut self,
        id: ComponentId,
        current_tab: ComponentId,
    ) -> RenderRequester {
        let requester = &self.requester;
        if !Self::uses_render_gate(id) {
            return requester.clone();
        }

        let visible = Arc::new(AtomicBool::new(id == current_tab));
        self.gates.insert(id, Arc::clone(&visible));
        requester.scoped(visible)
    }

    fn uses_render_gate(id: ComponentId) -> bool {
        // Only tabs with continuously running streams need render gates.
        matches!(id, ComponentId::Overview | ComponentId::Connections | ComponentId::Logs)
    }

    fn set_render_gate(&mut self, id: ComponentId, visible: bool) {
        if let Some(gate) = self.gates.get(&id) {
            gate.store(visible, Ordering::Release);
        }
    }

    pub(super) fn switch_tab(&mut self, from: ComponentId, to: ComponentId) {
        if from != to {
            self.set_render_gate(from, false);
            self.set_render_gate(to, true);
        }
    }

    pub(super) fn remove(&mut self, id: ComponentId) {
        self.gates.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use futures_util::FutureExt;

    use super::*;
    use crate::app::RenderScheduler;

    fn has_render_request(scheduler: &RenderScheduler) -> bool {
        scheduler.wait_for_request().now_or_never().is_some()
    }

    #[test]
    fn stream_tab_requester_follows_tab_visibility() {
        let scheduler = RenderScheduler::default();
        let mut requesters = TabRenderRequesters::new(scheduler.requester());

        let logs = requesters.requester_for(ComponentId::Logs, ComponentId::Rules);
        logs.request_render();
        assert!(!has_render_request(&scheduler));

        requesters.switch_tab(ComponentId::Rules, ComponentId::Logs);
        logs.request_render();
        assert!(has_render_request(&scheduler));

        requesters.switch_tab(ComponentId::Logs, ComponentId::Rules);
        logs.request_render();
        assert!(!has_render_request(&scheduler));
    }

    #[test]
    fn non_stream_tab_requester_is_not_gated() {
        let scheduler = RenderScheduler::default();
        let mut requesters = TabRenderRequesters::new(scheduler.requester());

        let rules = requesters.requester_for(ComponentId::Rules, ComponentId::Logs);
        requesters.switch_tab(ComponentId::Logs, ComponentId::Overview);

        rules.request_render();
        assert!(has_render_request(&scheduler));
    }
}
