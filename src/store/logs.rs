use std::borrow::Cow;
use std::num::NonZeroUsize;
use std::string::ToString;
use std::sync::{Arc, Mutex, RwLock};

use nucleo_matcher::Matcher;
use ringbuffer::{AllocRingBuffer, RingBuffer};

use crate::models::Log;
use crate::utils::columns::ColDef;
use crate::utils::row_filter::RowFilter;

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

    pub fn compute_view(&self, pattern: Option<&str>) {
        let buffer = self.buffer.read().unwrap();

        let mut matcher = self.matcher.lock().unwrap();
        let filtered = RowFilter::new(buffer.iter(), &mut matcher, pattern, LOG_COLS.iter());
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
