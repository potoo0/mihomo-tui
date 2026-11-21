use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;

/// A clonable rendering notification handle, independent of business actions.
#[derive(Clone, Debug)]
pub struct RenderRequester {
    notify: Arc<Notify>,
    visible: Option<Arc<AtomicBool>>,
}

impl RenderRequester {
    pub fn new(notify: Arc<Notify>) -> Self {
        Self { notify, visible: None }
    }

    /// Return a requester that only notifies while `visible` is true.
    pub fn scoped(&self, visible: Arc<AtomicBool>) -> Self {
        Self { visible: Some(visible), ..self.clone() }
    }

    /// Commit visible state before calling this. Repeated requests share one
    /// pending notification; a request during drawing schedules a later frame.
    pub fn request_render(&self) {
        if self.visible.as_ref().is_some_and(|visible| !visible.load(Ordering::Acquire)) {
            return;
        }
        self.notify.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use futures_util::FutureExt;

    use super::*;

    #[test]
    fn scoped_requester_only_notifies_while_visible() {
        let notify = Arc::new(Notify::new());
        let visible = Arc::new(AtomicBool::new(false));
        let requester = RenderRequester::new(Arc::clone(&notify)).scoped(Arc::clone(&visible));

        requester.request_render();
        assert!(notify.notified().now_or_never().is_none());

        visible.store(true, Ordering::Release);
        requester.request_render();
        assert!(notify.notified().now_or_never().is_some());
    }
}
