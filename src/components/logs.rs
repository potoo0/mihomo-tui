use std::borrow::Cow;
use std::string::ToString;
use std::sync::{Arc, RwLock};

use circular_buffer::CircularBuffer;
use fuzzy_matcher::skim::SkimMatcherV2;

use crate::components::LOGS_BUFFER_SIZE;
use crate::models::Log;
use crate::utils::columns::ColDef;
use crate::utils::row_filter::RowFilter;

#[derive(Default)]
pub struct Logs {
    matcher: Arc<SkimMatcherV2>,

    buffer: RwLock<CircularBuffer<LOGS_BUFFER_SIZE, Arc<Log>>>,
    view: RwLock<CircularBuffer<LOGS_BUFFER_SIZE, Arc<Log>>>,
}

impl Logs {
    pub fn push(&self, record: Log) {
        let mut guard = self.buffer.write().unwrap();
        guard.push_back(Arc::new(record));
    }

    pub fn compute_view(&self, pattern: Option<&str>) {
        let buffer = self.buffer.read().unwrap();

        let matcher = self.matcher.as_ref();
        let filtered = RowFilter::new(buffer.iter(), matcher, pattern, LOG_COLS);
        let mut guard = self.view.write().unwrap();
        guard.clear();
        filtered.for_each(|v| {
            guard.push_back(v);
        });
    }

    pub fn view(&self) -> Vec<Arc<Log>> {
        self.view.read().unwrap().to_vec()
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
