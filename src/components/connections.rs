use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use circular_buffer::CircularBuffer;
use fuzzy_matcher::skim::SkimMatcherV2;
use serde_json::Value;

use crate::components::CONNS_BUFFER_SIZE;
use crate::components::state::SearchState;
use crate::models::Connection;
use crate::utils::byte_size::human_bytes;
use crate::utils::columns::{ColDef, SortKey};
use crate::utils::row_filter::RowFilter;

#[derive(Default)]
pub struct Connections {
    matcher: Arc<SkimMatcherV2>,

    buffer: RwLock<CircularBuffer<CONNS_BUFFER_SIZE, Arc<Connection>>>,
    view: RwLock<CircularBuffer<CONNS_BUFFER_SIZE, Arc<Connection>>>,
    last_bytes: Mutex<HashMap<Arc<str>, (u64, u64)>>, // id -> (upload, download)
}

impl Connections {
    pub fn push(&self, capture_mode: bool, records: Vec<Connection>) {
        let mut guard = self.buffer.write().unwrap();
        // todo implement capture mode: deduplication and push
        if !capture_mode {
            guard.clear();
        }
        let mut map = HashMap::with_capacity(records.len());
        let mut map_guard = self.last_bytes.lock().unwrap();
        records.into_iter().for_each(|mut item| {
            let key = Arc::from(item.id.as_str());
            map.insert(Arc::clone(&key), (item.upload, item.download));
            if let Some((up, down)) = map_guard.get(&key) {
                item.upload_rate = item.upload.saturating_sub(*up);
                item.download_rate = item.download.saturating_sub(*down);
            }
            guard.push_back(Arc::new(item));
        });
        *map_guard = map;
    }

    pub fn compute_view(&self, search_state: &SearchState) {
        let buffer = self.buffer.read().unwrap();

        let pattern = search_state.pattern.as_deref();
        let matcher = self.matcher.as_ref();
        let filtered = RowFilter::new(buffer.iter(), matcher, pattern, CONNECTION_COLS);

        if let Some(sort) = search_state.sort
            && let Some(col_def) = CONNECTION_COLS.get(sort.col)
            && col_def.sortable
        {
            let mut v: Vec<Arc<Connection>> = filtered.collect();
            v.sort_by(|a, b| col_def.ordering(a, b, sort.dir));
            let mut guard = self.view.write().unwrap();
            guard.clear();
            guard.extend_from_slice(&v)
        } else {
            let mut guard = self.view.write().unwrap();
            guard.clear();
            filtered.for_each(|v| {
                guard.push_back(v);
            });
        }
    }

    pub fn view(&self) -> Vec<Arc<Connection>> {
        self.view.read().unwrap().to_vec()
    }

    pub fn get(&self, index: usize) -> Option<Arc<Connection>> {
        self.view.read().unwrap().get(index).cloned()
    }
}

pub static CONNECTION_COLS: &[ColDef<Connection>] = &[
    ColDef {
        id: "host",
        title: "Host",
        filterable: true,
        sortable: true,
        accessor: |c: &Connection| {
            let dst_port = match &c.metadata["destinationPort"] {
                Value::Number(number) => number
                    .as_u64()
                    .map(|v| Cow::Owned(format!("{v}")))
                    .unwrap_or_else(|| Cow::Borrowed("")),
                Value::String(str) => Cow::Borrowed(str.as_str()),
                _ => Cow::Borrowed(""),
            };
            if let Some(h) =
                c.metadata.get("host").and_then(Value::as_str).filter(|s| !s.is_empty())
            {
                return Cow::Owned(format!("{h}:{}", dst_port));
            }

            let dip = c.metadata.get("destinationIP").and_then(Value::as_str).unwrap_or("");
            let with_port = if dip.contains(':') {
                // IPv6
                format!("[{dip}]:{}", dst_port)
            } else {
                format!("{dip}:{}", dst_port)
            };

            Cow::Owned(with_port)
        },
        sort_key: None,
    },
    ColDef {
        id: "rule",
        title: "Rule",
        filterable: true,
        sortable: true,
        accessor: |c: &Connection| Cow::Borrowed(c.rule.as_str()),
        sort_key: None,
    },
    ColDef {
        id: "chains",
        title: "Chains",
        filterable: true,
        sortable: true,
        accessor: |c: &Connection| Cow::Owned(c.chains.join(" > ")),
        sort_key: None,
    },
    ColDef {
        id: "down_rate",
        title: "DownRate",
        filterable: false,
        sortable: true,
        accessor: |c: &Connection| Cow::Owned(human_bytes(c.download_rate as f64, Some("/s"))),
        sort_key: Some(|c: &Connection| SortKey::U64(c.download_rate)),
    },
    ColDef {
        id: "up_rate",
        title: "UpRate",
        filterable: false,
        sortable: true,
        accessor: |c: &Connection| Cow::Owned(human_bytes(c.upload_rate as f64, Some("/s"))),
        sort_key: Some(|c: &Connection| SortKey::U64(c.upload_rate)),
    },
    ColDef {
        id: "down_total",
        title: "DownTotal",
        filterable: false,
        sortable: true,
        accessor: |c: &Connection| Cow::Owned(human_bytes(c.download as f64, None)),
        sort_key: Some(|c: &Connection| SortKey::U64(c.download)),
    },
    ColDef {
        id: "up_total",
        title: "UpTotal",
        filterable: false,
        sortable: true,
        accessor: |c: &Connection| Cow::Owned(human_bytes(c.upload as f64, None)),
        sort_key: Some(|c: &Connection| SortKey::U64(c.upload)),
    },
    ColDef {
        id: "source_ip",
        title: "SourceIP",
        filterable: true,
        sortable: true,
        accessor: |c: &Connection| Cow::Borrowed(c.metadata["sourceIP"].as_str().unwrap_or("-")),
        sort_key: None,
    },
];
