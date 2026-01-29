use std::borrow::Cow;
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};

use fuzzy_matcher::skim::SkimMatcherV2;

use crate::models::Rule;
use crate::utils::columns::ColDef;
use crate::utils::row_filter::RowFilter;
use crate::utils::time::format_datetime;

#[derive(Default)]
pub struct Rules {
    matcher: Arc<SkimMatcherV2>,

    buffer: RwLock<Vec<Arc<Rule>>>,
    view: RwLock<Vec<Arc<Rule>>>,
}

impl Rules {
    pub fn push(&self, records: Vec<Rule>) {
        *self.buffer.write().unwrap() = records
            .into_iter()
            .map(|mut r| {
                if let Some(extra) = r.extra.as_mut() {
                    extra.hit_at_str = extra.hit_at.and_then(format_datetime);
                }
                if let (Some(extra), Some(_)) = (r.extra.as_ref(), r.index) {
                    r.disable_state.store(extra.disabled, Ordering::Relaxed);
                }
                Arc::new(r)
            })
            .collect();
    }

    pub fn compute_view(&self, pattern: Option<&str>) {
        let buffer = self.buffer.read().unwrap();

        let matcher = self.matcher.as_ref();
        let filtered = RowFilter::new(buffer.iter(), matcher, pattern, RULE_COLS);
        let mut guard = self.view.write().unwrap();
        guard.clear();
        filtered.for_each(|v| guard.push(v));
    }

    pub fn with_view<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Vec<Arc<Rule>>) -> R,
    {
        let guard = self.view.read().unwrap();
        f(&guard)
    }

    pub fn supports_disable(&self) -> bool {
        let records = self.buffer.read().unwrap();
        records.first().map(|v| v.supports_disable()).unwrap_or(false)
    }
}

pub static RULE_COLS: &[ColDef<Rule>] = &[
    ColDef {
        id: "index",
        title: "Index",
        filterable: false,
        sortable: false,
        accessor: |rule: &Rule| Cow::Owned(rule.index.map(|v| v.to_string()).unwrap_or("-".into())),
        sort_key: None,
    },
    ColDef {
        id: "rule",
        title: "Rule",
        filterable: true,
        sortable: false,
        accessor: |rule: &Rule| {
            let mut content = String::with_capacity(
                rule.r#type.len() + rule.payload.len() + rule.proxy.len() + 2,
            );
            content.push_str(&rule.r#type);
            if !rule.payload.is_empty() {
                content.push(',');
                content.push_str(&rule.payload);
            }
            content.push(',');
            content.push_str(&rule.proxy);
            Cow::Owned(content)
        },
        sort_key: None,
    },
    ColDef {
        id: "size",
        title: "Size",
        filterable: false,
        sortable: false,
        accessor: |rule: &Rule| {
            if rule.size <= -1 { Cow::Borrowed("-") } else { Cow::Owned(rule.size.to_string()) }
        },
        sort_key: None,
    },
    ColDef {
        id: "disabled",
        title: "Disabled",
        filterable: false,
        sortable: false,
        accessor: |rule: &Rule| match rule.extra {
            Some(ref extra) => {
                let backend = extra.disabled;
                let ui = rule.disable_state.load(Ordering::Relaxed);

                match (backend, ui) {
                    (true, true) => Cow::Borrowed("Y"),
                    (false, false) => Cow::Borrowed("N"),
                    (true, false) => Cow::Borrowed("Y -> N"),
                    (false, true) => Cow::Borrowed("N -> Y"),
                }
            }
            None => Cow::Borrowed("-"),
        },
        sort_key: None,
    },
    ColDef {
        id: "hits",
        title: "Hits",
        filterable: false,
        sortable: false,
        accessor: |rule: &Rule| {
            Cow::Owned(rule.extra.as_ref().map(|v| v.hit_count.to_string()).unwrap_or("-".into()))
        },
        sort_key: None,
    },
    ColDef {
        id: "hit_at",
        title: "HitAt",
        filterable: false,
        sortable: false,
        accessor: |rule: &Rule| {
            Cow::Borrowed(rule.extra.as_ref().and_then(|v| v.hit_at_str.as_deref()).unwrap_or("-"))
        },
        sort_key: None,
    },
];
