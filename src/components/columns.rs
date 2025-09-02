#[allow(dead_code)]
use std::borrow::Cow;
use std::cmp::Ordering;

use serde_json::Value;

use crate::models::Connection;
use crate::utils::byte_size::human_bytes;

pub struct ColDef<T> {
    pub id: &'static str,
    pub title: &'static str,
    pub filterable: bool,
    pub sortable: bool,
    /// value accessor, used in cell rendering and filtering
    pub accessor: for<'a> fn(&'a T) -> Cow<'a, str>,
    /// sort key, optional. If None, use the string from accessor for sorting
    pub sort_key: Option<fn(&T) -> SortKey>,
}

impl<T> ColDef<T> {
    /// Compare two items based on this column definition
    #[inline]
    pub fn cmp(&self, a: &T, b: &T) -> Ordering {
        if let Some(f) = self.sort_key {
            f(a).cmp(&f(b))
        } else {
            let sa = (self.accessor)(a);
            let sb = (self.accessor)(b);
            sa.as_ref().cmp(sb.as_ref()) // use as_ref to avoid allocating
        }
    }

    #[inline]
    pub fn ordering(&self, a: &T, b: &T, desc: bool) -> Ordering {
        let ord = self.cmp(a, b);
        if desc { ord.reverse() } else { ord }
    }
}

#[derive(Debug, Clone)]
pub enum SortKey {
    Str(String),
    U64(u64),
    F64(f64),
}

impl SortKey {
    pub fn cmp(&self, other: &Self) -> Ordering {
        use SortKey::*;
        match (self, other) {
            (U64(a), U64(b)) => a.cmp(b),
            (F64(a), F64(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (Str(a), Str(b)) => a.cmp(b),
            (a, b) => format!("{a:?}").cmp(&format!("{b:?}")),
        }
    }
}

pub static CONNECTION_COLS: &[ColDef<Connection>] = &[
    ColDef {
        id: "host",
        title: "Host",
        filterable: true,
        sortable: true,
        accessor: |c: &Connection| {
            let dport = match &c.metadata["destinationPort"] {
                Value::Number(number) => number
                    .as_u64()
                    .map(|v| Cow::Owned(format!("{v}")))
                    .unwrap_or_else(|| Cow::Borrowed("")),
                Value::String(str) => Cow::Borrowed(str.as_str()),
                _ => Cow::Borrowed(""),
            };
            if let Some(h) = c
                .metadata
                .get("host")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                return Cow::Owned(format!("{h}:{}", dport));
            }

            let dip = c
                .metadata
                .get("destinationIP")
                .and_then(Value::as_str)
                .unwrap_or("");
            let with_port = if dip.contains(':') {
                // IPv6
                format!("[{dip}]:{}", dport)
            } else {
                format!("{dip}:{}", dport)
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
        accessor: |_: &Connection| Cow::Borrowed("-"),
        sort_key: None,
    },
    ColDef {
        id: "up_rate",
        title: "UpRate",
        filterable: false,
        sortable: true,
        accessor: |_: &Connection| Cow::Borrowed("-"),
        sort_key: None,
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
