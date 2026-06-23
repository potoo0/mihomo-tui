use std::borrow::Cow;
use std::num::NonZeroUsize;
use std::string::ToString;
use std::sync::{Arc, Mutex, RwLock};

use nucleo_matcher::Matcher;
use ringbuffer::{AllocRingBuffer, RingBuffer};

use crate::models::Log;
use crate::utils::columns::ColDef;
use crate::utils::filter::{FilterPattern, RowFilter};

pub struct Logs {
    matcher: Mutex<Matcher>,

    buffer: RwLock<AllocRingBuffer<Arc<Log>>>,
    view: RwLock<AllocRingBuffer<Arc<Log>>>,
}

impl Logs {
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            matcher: Default::default(),
            buffer: RwLock::new(AllocRingBuffer::new(capacity.get())),
            view: RwLock::new(AllocRingBuffer::new(capacity.get())),
        }
    }

    pub fn push(&self, record: Log) {
        let mut guard = self.buffer.write().unwrap();
        guard.enqueue(Arc::new(record));
    }

    pub fn push_and_update_view(&self, record: Log, pattern: Option<&FilterPattern>) {
        let record = Arc::new(record);
        let removed = {
            let mut guard = self.buffer.write().unwrap();
            guard.enqueue(Arc::clone(&record))
        };

        let matches = {
            let mut matcher = self.matcher.lock().unwrap();
            RowFilter::new(
                std::iter::once(&record),
                &mut matcher,
                pattern.map(FilterPattern::expr),
                LOG_COLS.iter(),
            )
            .next()
            .is_some()
        };

        let mut guard = self.view.write().unwrap();
        // Keep the filtered view in sync when the ring buffer evicts its oldest record.
        if let Some(removed) = removed
            && guard.front().is_some_and(|log| Arc::ptr_eq(log, &removed))
        {
            guard.dequeue();
        }
        if matches {
            guard.enqueue(record);
        }
    }

    pub fn compute_view(&self, pattern: Option<&FilterPattern>) {
        let buffer = self.buffer.read().unwrap();

        let mut matcher = self.matcher.lock().unwrap();
        let filtered = RowFilter::new(
            buffer.iter(),
            &mut matcher,
            pattern.map(FilterPattern::expr),
            LOG_COLS.iter(),
        );
        let mut guard = self.view.write().unwrap();
        guard.clear();
        guard.extend(filtered)
    }

    pub fn with_view<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&AllocRingBuffer<Arc<Log>>) -> R,
    {
        let guard = self.view.read().unwrap();
        f(&guard)
    }
}

pub static LOG_COLS: &[ColDef<Log>] = &[
    ColDef {
        id: "level",
        title: "Level",
        filterable: true,
        sortable: false,
        accessor: |c: &Log| Cow::Owned(c.r#type.to_string()),
        sort_key: None,
    },
    ColDef {
        id: "content",
        title: "Content",
        filterable: true,
        sortable: false,
        accessor: |c: &Log| Cow::Borrowed(c.payload.as_str()),
        sort_key: None,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::LogLevel;

    fn log(payload: &str) -> Log {
        Log { r#type: LogLevel::Info, payload: payload.to_owned() }
    }

    fn payloads(store: &Logs) -> Vec<String> {
        store.with_view(|records| records.iter().map(|record| record.payload.clone()).collect())
    }

    #[test]
    fn push_and_update_view_filters_new_record() {
        let store = Logs::new(NonZeroUsize::new(4).unwrap());
        let pattern = FilterPattern::new("foo".to_owned());

        store.push_and_update_view(log("foo one"), pattern.as_ref());
        store.push_and_update_view(log("bar two"), pattern.as_ref());
        store.push_and_update_view(log("foo three"), pattern.as_ref());

        assert_eq!(payloads(&store), ["foo one", "foo three"]);
    }

    #[test]
    fn push_and_update_view_removes_expired_filtered_record() {
        let store = Logs::new(NonZeroUsize::new(2).unwrap());
        let pattern = FilterPattern::new("foo".to_owned());

        store.push_and_update_view(log("foo one"), pattern.as_ref());
        store.push_and_update_view(log("bar two"), pattern.as_ref());
        store.push_and_update_view(log("foo three"), pattern.as_ref());

        assert_eq!(payloads(&store), ["foo three"]);
    }
}
