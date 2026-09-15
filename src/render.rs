use std::future::pending;
use std::sync::Arc;
use std::time::Duration;

use futures_util::FutureExt;
use tokio::sync::Notify;
use tokio::time::{Instant, sleep_until};

const MAX_RENDER_FPS: u64 = 30;
// Round up so the minimum interval never exceeds the maximum frame rate.
const MIN_RENDER_INTERVAL: Duration =
    Duration::from_nanos(1_000_000_000u64.div_ceil(MAX_RENDER_FPS));

/// A clonable rendering notification handle, independent of business actions.
#[derive(Clone, Debug)]
pub struct RenderRequester {
    notify: Arc<Notify>,
}

impl RenderRequester {
    /// Commit visible state before calling this. Repeated requests share one
    /// pending notification; a request during drawing schedules a later frame.
    pub fn request_render(&self) {
        self.notify.notify_one();
    }
}

#[derive(Default)]
pub struct RenderScheduler {
    notify: Arc<Notify>,
    requested: bool,
    last_rendered_at: Option<Instant>,
}

impl RenderScheduler {
    pub fn requester(&self) -> RenderRequester {
        RenderRequester { notify: Arc::clone(&self.notify) }
    }

    /// Wait for and consume a rendering notification.
    pub async fn wait_for_request(&self) {
        self.notify.notified().await;
    }

    /// Consume a request already covered by the current rendering batch.
    pub fn consume_pending_request(&self) {
        let _ = self.notify.notified().now_or_never();
    }

    pub fn request(&mut self) {
        self.requested = true;
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.requested
            .then(|| self.last_rendered_at.map_or_else(Instant::now, |at| at + MIN_RENDER_INTERVAL))
    }

    pub fn is_due(&self) -> bool {
        self.deadline().is_some_and(|at| at <= Instant::now())
    }

    // Call only after a successful draw. Requests received during that draw
    // remain in Notify and will mark the next batch dirty.
    pub fn rendered(&mut self) {
        self.requested = false;
        self.last_rendered_at = Some(Instant::now());
    }

    pub async fn wait(deadline: Option<Instant>) {
        match deadline {
            Some(at) => sleep_until(at).await,
            None => pending().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn idle_has_no_deadline_and_first_frame_is_immediate() {
        let mut scheduler = RenderScheduler::default();
        assert!(scheduler.deadline().is_none());
        assert!(!scheduler.is_due());
        scheduler.request();
        assert!(scheduler.is_due());
        scheduler.rendered();
        assert!(scheduler.deadline().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn last_input_gets_a_frame_without_another_event_or_floor_tick() {
        let mut scheduler = RenderScheduler::default();
        scheduler.rendered();
        let previous_frame = Instant::now();
        tokio::time::advance(Duration::from_millis(10)).await;
        scheduler.request();
        assert!(!scheduler.is_due());
        let deadline = scheduler.deadline().unwrap();
        assert_eq!(deadline, previous_frame + MIN_RENDER_INTERVAL);

        // More input must not debounce the deadline further into the future.
        tokio::time::advance(Duration::from_millis(10)).await;
        scheduler.request();
        assert_eq!(scheduler.deadline(), Some(deadline));
        RenderScheduler::wait(scheduler.deadline()).await;
        assert!(scheduler.is_due());
        assert!(Instant::now() - previous_frame < Duration::from_millis(35));
        scheduler.rendered();
        assert!(scheduler.deadline().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn continuous_requests_obey_frame_limit_and_flush_the_last_update() {
        let mut scheduler = RenderScheduler::default();
        let mut frames = Vec::new();
        let mut state = 0;
        while state < 100 {
            state += 1;
            scheduler.request();
            if scheduler.is_due() {
                frames.push((Instant::now(), state));
                scheduler.rendered();
            }
            tokio::time::advance(Duration::from_millis(1)).await;
        }
        RenderScheduler::wait(scheduler.deadline()).await;
        assert!(scheduler.is_due());
        frames.push((Instant::now(), state));
        scheduler.rendered();
        assert_eq!(frames.last().unwrap().1, 100);
        assert!(frames.len() >= 3);
        assert!(frames.windows(2).all(|pair| pair[1].0 - pair[0].0 >= MIN_RENDER_INTERVAL));
        assert!(scheduler.deadline().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn repeated_requests_coalesce_without_postponing_the_frame() {
        let mut scheduler = RenderScheduler::default();
        let requester = scheduler.requester();
        scheduler.rendered();
        for _ in 0..10_000 {
            requester.request_render();
        }
        scheduler.wait_for_request().await;
        scheduler.request();
        let deadline = scheduler.deadline().unwrap();
        assert!(scheduler.wait_for_request().now_or_never().is_none());

        tokio::time::advance(Duration::from_millis(10)).await;
        requester.request_render();
        scheduler.wait_for_request().await;
        scheduler.request();
        assert_eq!(scheduler.deadline(), Some(deadline));
        RenderScheduler::wait(Some(deadline)).await;
        assert!(scheduler.is_due());
    }

    #[test]
    fn pending_request_can_be_consumed_by_the_current_batch() {
        let scheduler = RenderScheduler::default();
        let requester = scheduler.requester();
        requester.request_render();
        scheduler.consume_pending_request();
        assert!(scheduler.wait_for_request().now_or_never().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn background_request_wakes_idle_and_request_during_draw_survives() {
        let mut scheduler = RenderScheduler::default();
        let requester = scheduler.requester();
        let producer = requester.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            producer.request_render();
        });
        let start = Instant::now();
        tokio::select! {
            _ = scheduler.wait_for_request() => {},
            _ = tokio::time::sleep(Duration::from_secs(1)) => panic!("waited for fallback"),
        }
        scheduler.request();
        assert!(Instant::now() - start < Duration::from_secs(1));
        assert!(scheduler.is_due());

        // Simulate a producer committing new state while the first frame draws.
        requester.request_render();
        scheduler.rendered();
        scheduler.wait_for_request().await;
        scheduler.request();
        assert!(!scheduler.is_due());
        RenderScheduler::wait(scheduler.deadline()).await;
        assert!(scheduler.is_due());
    }

    #[tokio::test(start_paused = true)]
    async fn cancelling_a_wait_preserves_the_next_notification() {
        let mut scheduler = RenderScheduler::default();
        let requester = scheduler.requester();
        assert!(scheduler.wait_for_request().now_or_never().is_none());
        requester.request_render();
        assert!(scheduler.wait_for_request().now_or_never().is_some());
        scheduler.request();
        assert!(scheduler.is_due());
    }
}
