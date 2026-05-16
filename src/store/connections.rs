use std::borrow::Cow;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, RwLock};

use const_format::concatcp;
use indexmap::IndexMap;
use nucleo_matcher::Matcher;
use ringbuffer::{AllocRingBuffer, RingBuffer};
use serde_json::Value;

use crate::models::Connection;
use crate::store::CONNS_BUFFER_SIZE;
use crate::store::query::QueryState;
use crate::utils::byte_size::human_bytes;
use crate::utils::columns::{ColDef, SortKey};
use crate::utils::row_filter::RowFilter;
use crate::utils::symbols::dot;

pub struct Connections {
    matcher: Mutex<Matcher>,

    buffer: RwLock<AllocRingBuffer<Arc<Connection>>>,
    view: RwLock<AllocRingBuffer<Arc<Connection>>>,
    last_bytes: Mutex<HashMap<Arc<str>, (u64, u64)>>, // id -> (upload, download)
}

impl Connections {
    pub fn new(capacity: Option<NonZeroUsize>) -> Self {
        let capacity = capacity.map(NonZeroUsize::get).unwrap_or(CONNS_BUFFER_SIZE);
        Self {
            matcher: Default::default(),
            buffer: RwLock::new(AllocRingBuffer::new(capacity)),
            view: RwLock::new(AllocRingBuffer::new(capacity)),
            last_bytes: Default::default(),
        }
    }

    pub fn push(&self, capture_mode: bool, records: Vec<Connection>) {
        let mut guard = self.buffer.write().unwrap();
        let mut history: IndexMap<Arc<str>, Arc<Connection>> = if capture_mode {
            guard.iter().cloned().map(|p| (p.id.as_str().into(), p)).collect()
        } else {
            Default::default()
        };
        guard.clear();
        {
            let mut map = HashMap::with_capacity(records.len());
            let mut map_guard = self.last_bytes.lock().unwrap();
            records.into_iter().for_each(|mut item| {
                let key = Arc::from(item.id.as_str());
                history.shift_remove(&key);
                map.insert(Arc::clone(&key), (item.upload, item.download));
                if let Some((up, down)) = map_guard.get(&key) {
                    item.upload_rate = item.upload.saturating_sub(*up);
                    item.download_rate = item.download.saturating_sub(*down);
                }
                guard.enqueue(Arc::new(item));
            });
            *map_guard = map;
        }
        history.into_values().for_each(|v| {
            v.inactive.store(true, Ordering::Relaxed);
            _ = guard.enqueue(v);
        });
    }

    pub fn compute_view(&self, query_state: &QueryState) {
        let buffer = self.buffer.read().unwrap();

        let pattern = query_state.pattern.as_deref();
        let mut matcher = self.matcher.lock().unwrap();
        let filtered = RowFilter::new(buffer.iter(), &mut matcher, pattern, CONNECTION_COLS);

        if let Some(sort) = query_state.sort
            && let Some(col_def) = CONNECTION_COLS.get(sort.col)
            && col_def.sortable
        {
            let mut v: Vec<Arc<Connection>> = filtered.collect();
            v.sort_by(|a, b| col_def.ordering(a, b, sort.dir));
            let mut guard = self.view.write().unwrap();
            guard.clear();
            guard.extend(v)
            // guard.extend_from_slice(&v)
        } else {
            let mut guard = self.view.write().unwrap();
            guard.clear();
            filtered.for_each(|v| _ = guard.enqueue(v));
        }
    }

    pub fn with_view<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&AllocRingBuffer<Arc<Connection>>) -> R,
    {
        let guard = self.view.read().unwrap();
        f(&guard)
    }

    pub fn get(&self, index: usize) -> Option<Arc<Connection>> {
        self.view.read().unwrap().get(index).cloned()
    }
}

pub fn find_sortable_connection_col(field: &str) -> Option<usize> {
    CONNECTION_COLS
        .iter()
        .enumerate()
        .find(|(_, def)| def.sortable && def.title.eq_ignore_ascii_case(field))
        .map(|(idx, _)| idx)
}

pub static CONNECTION_COLS: &[ColDef<Connection>] = &[
    ColDef {
        id: "alive",
        title: "Alive",
        filterable: false,
        sortable: true,
        accessor: |c: &Connection| {
            let alive = !c.inactive.load(Ordering::Relaxed);
            Cow::Borrowed(if alive {
                concatcp!(" ", dot::GREEN_LARGE)
            } else {
                concatcp!(" ", dot::RED_LARGE)
            })
        },
        sort_key: Some(|c: &Connection| SortKey::Bool(!c.inactive.load(Ordering::Relaxed))),
    },
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
        accessor: |c: &Connection| {
            // Reverse to display in correct order
            let chains: Vec<&str> = c.chains.iter().rev().map(String::as_str).collect();
            Cow::Owned(chains.join(" > "))
        },
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

#[cfg(test)]
mod tests {
    use ringbuffer::{AllocRingBuffer, RingBuffer};

    #[test]
    fn test_ring_buffer() {
        let mut buffer = AllocRingBuffer::new(2);
        buffer.enqueue(1);
        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer.to_vec(), vec![1]);
        buffer.enqueue(2);
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.to_vec(), vec![1, 2]);
        buffer.enqueue(3);
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.to_vec(), vec![2, 3]);
        buffer.enqueue(4);
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.to_vec(), vec![3, 4]);
    }
}
