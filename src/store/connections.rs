use std::borrow::Cow;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, RwLock};

use const_format::concatcp;
use indexmap::IndexMap;
use nucleo_matcher::Matcher;
use ratatui::layout::Constraint;
use ringbuffer::{AllocRingBuffer, RingBuffer};
use serde_json::Value;

use crate::models::Connection;
use crate::store::connections_setting::ConnectionsSetting;
use crate::utils::byte_size::human_bytes;
use crate::utils::columns::{ColDef, SortKey, TextResolver, find_index_ignore_ascii_case};
use crate::utils::row_filter::RowFilter;
use crate::utils::symbols::dot;

pub struct Connections {
    matcher: Mutex<Matcher>,

    buffer: RwLock<AllocRingBuffer<Arc<Connection>>>,
    view: RwLock<AllocRingBuffer<Arc<Connection>>>,
    last_bytes: Mutex<HashMap<Arc<str>, (u64, u64)>>, // id -> (upload, download)
}

impl Connections {
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            matcher: Default::default(),
            buffer: RwLock::new(AllocRingBuffer::new(capacity.get())),
            view: RwLock::new(AllocRingBuffer::new(capacity.get())),
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

    pub fn compute_view(&self) {
        let setting = ConnectionsSetting::snapshot();
        let query_state = &setting.query_state;
        let buffer = self.buffer.read().unwrap();

        let pattern = query_state.pattern.as_deref();
        let mut matcher = self.matcher.lock().unwrap();
        let text_resolver = SourceIpAliasTextResolver { source_ip_alias: &setting.source_ip_alias };
        let filtered = RowFilter::new(
            buffer.iter(),
            &mut matcher,
            pattern,
            setting.columns.iter().filter_map(|&idx| CONNECTION_COLS.get(idx)),
        )
        .with_text_resolver(&text_resolver);

        if let Some(sort) = query_state.sort
            && let Some(col_def) =
                setting.columns.get(sort.col).and_then(|&col| CONNECTION_COLS.get(col))
            && col_def.sortable
        {
            let mut v: Vec<Arc<Connection>> = filtered.collect();
            v.sort_by(|a, b| col_def.ordering_with_text_resolver(a, b, sort.dir, &text_resolver));
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

    pub fn source_ips(&self) -> Vec<String> {
        let mut source_ips = self
            .buffer
            .read()
            .unwrap()
            .iter()
            .filter_map(|connection| {
                connection.metadata.get("sourceIP").and_then(Value::as_str).map(str::trim)
            })
            .filter(|source_ip| !source_ip.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        source_ips.sort_unstable();
        source_ips.dedup();
        source_ips
    }
}

pub(crate) struct SourceIpAliasTextResolver<'a> {
    pub(crate) source_ip_alias: &'a HashMap<String, String>,
}

impl TextResolver<Connection> for SourceIpAliasTextResolver<'_> {
    fn resolve<'row>(
        &self,
        col: &ColDef<Connection>,
        _connection: &'row Connection,
        text: Cow<'row, str>,
    ) -> Cow<'row, str> {
        if col.id != "source_ip" {
            return text;
        }

        self.source_ip_alias
            .get(text.as_ref())
            .map(|alias| Cow::Owned(alias.clone()))
            .unwrap_or(text)
    }
}

/// Index of the runtime-only alive indicator column.
///
/// This column is added for capture mode display and is not user-configurable.
pub const ALIVE_COLUMN_INDEX: usize = find_index_ignore_ascii_case(CONNECTION_COLS, "Alive");

pub fn with_alive_column(columns: impl IntoIterator<Item = usize>) -> Vec<usize> {
    let mut columns = columns.into_iter().collect::<Vec<_>>();
    if !columns.contains(&ALIVE_COLUMN_INDEX) {
        columns.insert(0, ALIVE_COLUMN_INDEX);
    }
    columns
}

/// Column definitions for the connections table.
///
/// User config stores column IDs, which are parsed into runtime indices in this
/// slice. `ALIVE_COLUMN_INDEX` is runtime-only and must stay excluded from user
/// column settings.
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

pub static CONNECTION_COL_CONSTRAINTS: &[Constraint] = &[
    Constraint::Length(6),
    Constraint::Min(30),
    Constraint::Max(15),
    Constraint::Min(10),
    Constraint::Max(15),
    Constraint::Max(15),
    Constraint::Max(15),
    Constraint::Max(15),
    Constraint::Max(20),
];

/// Default runtime columns for the connections table.
///
/// Runtime columns include `ALIVE_COLUMN_INDEX`; user-configurable columns must
/// filter it out before showing or persisting user choices.
pub const DEFAULT_CONNECTION_COL_INDICES: &[usize] = &[0, 1, 2, 3, 4, 5, 6, 7, 8];

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::num::NonZeroUsize;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex as StdMutex, OnceLock};

    use ringbuffer::{AllocRingBuffer, RingBuffer};
    use serde_json::json;

    use super::*;
    use crate::models::sort::{SortDir, SortSpec};
    use crate::store::query::QueryState;

    fn settings_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| StdMutex::new(())).lock().unwrap()
    }

    fn connection(id: &str, source_ip: Option<&str>) -> Connection {
        let metadata =
            source_ip.map_or_else(|| json!({}), |source_ip| json!({ "sourceIP": source_ip }));
        Connection {
            id: id.into(),
            metadata,
            upload: 0,
            download: 0,
            start: String::new(),
            chains: Vec::new(),
            rule: String::new(),
            rule_payload: String::new(),
            inactive: Arc::new(AtomicBool::new(false)),
            upload_rate: 0,
            download_rate: 0,
        }
    }

    fn connection_col_index(id: &str) -> usize {
        CONNECTION_COLS
            .iter()
            .position(|col| col.id == id)
            .unwrap_or_else(|| panic!("connection column {id:?} should exist"))
    }

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

    #[test]
    fn source_ips_returns_sorted_unique_non_empty_values() {
        let store = Connections::new(NonZeroUsize::new(10).unwrap());

        store.push(
            false,
            vec![
                connection("1", Some("10.0.0.2")),
                connection("2", Some("10.0.0.1")),
                connection("3", Some("10.0.0.2")),
                connection("4", Some("")),
                connection("5", None),
            ],
        );

        assert_eq!(store.source_ips(), vec!["10.0.0.1", "10.0.0.2"]);
    }

    #[test]
    fn filters_only_visible_columns() {
        let _guard = settings_test_lock();
        let store = Connections::new(NonZeroUsize::new(10).unwrap());
        let mut conn = connection("1", None);
        conn.rule = "secret-rule".to_string();
        store.push(false, vec![conn]);

        let columns = with_alive_column([connection_col_index("host")]);
        ConnectionsSetting::update(|setting| {
            setting.columns = columns.clone();
            setting.query_state = QueryState::new(columns.len());
            setting.query_state.pattern = Some("secret-rule".to_string());
            setting.source_ip_alias.clear();
        });
        store.compute_view();
        assert_eq!(store.with_view(|records| records.len()), 0);

        let columns = with_alive_column([connection_col_index("rule")]);
        ConnectionsSetting::update(|setting| {
            setting.columns = columns.clone();
            setting.query_state = QueryState::new(columns.len());
            setting.query_state.pattern = Some("secret-rule".to_string());
            setting.source_ip_alias.clear();
        });
        store.compute_view();
        assert_eq!(
            store.with_view(|records| {
                records.iter().map(|connection| connection.id.to_string()).collect::<Vec<_>>()
            }),
            vec!["1"]
        );

        ConnectionsSetting::update(|setting| {
            let columns = DEFAULT_CONNECTION_COL_INDICES.to_vec();
            setting.columns = columns.clone();
            setting.query_state = QueryState::new(columns.len());
            setting.source_ip_alias.clear();
        });
    }

    #[test]
    fn source_ip_alias_filters_and_sorts_view() {
        let _guard = settings_test_lock();
        let store = Connections::new(NonZeroUsize::new(10).unwrap());
        store.push(
            false,
            vec![connection("1", Some("10.0.0.2")), connection("2", Some("10.0.0.1"))],
        );

        let columns = DEFAULT_CONNECTION_COL_INDICES.to_vec();
        ConnectionsSetting::update(|setting| {
            setting.columns = columns.clone();
            setting.query_state = QueryState::new(columns.len());
            setting.source_ip_alias =
                HashMap::from([("10.0.0.1".to_string(), "phone".to_string())]);
        });

        ConnectionsSetting::update(|setting| {
            setting.query_state.pattern = Some("phone".to_string());
            setting.query_state.sort = None;
        });
        store.compute_view();
        assert_eq!(
            store.with_view(|records| {
                records.iter().map(|connection| connection.id.to_string()).collect::<Vec<_>>()
            }),
            vec!["2"]
        );

        let columns = with_alive_column([connection_col_index("host")]);
        ConnectionsSetting::update(|setting| {
            setting.columns = columns.clone();
            setting.query_state = QueryState::new(columns.len());
            setting.query_state.pattern = Some("phone".to_string());
        });
        store.compute_view();
        assert_eq!(store.with_view(|records| records.len()), 0);

        let columns = DEFAULT_CONNECTION_COL_INDICES.to_vec();
        let source_ip_visible_col =
            columns.iter().position(|&col| CONNECTION_COLS[col].id == "source_ip").unwrap();
        ConnectionsSetting::update(|setting| {
            setting.columns = columns.clone();
            setting.query_state = QueryState::new(columns.len());
            setting.query_state.pattern = None;
            setting.query_state.sort =
                Some(SortSpec { col: source_ip_visible_col, dir: SortDir::Asc });
        });
        store.compute_view();
        assert_eq!(
            store.with_view(|records| {
                records.iter().map(|connection| connection.id.to_string()).collect::<Vec<_>>()
            }),
            vec!["1", "2"]
        );

        let source_ip_col = CONNECTION_COLS.iter().find(|col| col.id == "source_ip").unwrap();
        assert_eq!((source_ip_col.accessor)(&connection("3", Some("10.0.0.1"))), "10.0.0.1");

        ConnectionsSetting::update(|setting| {
            let columns = DEFAULT_CONNECTION_COL_INDICES.to_vec();
            setting.columns = columns.clone();
            setting.query_state = QueryState::new(columns.len());
            setting.source_ip_alias.clear();
        });
    }
}
