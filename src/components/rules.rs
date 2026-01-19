use std::borrow::Cow;
use std::sync::{Arc, RwLock};

use fuzzy_matcher::skim::SkimMatcherV2;

use crate::models::Rule;
use crate::utils::columns::ColDef;
use crate::utils::row_filter::RowFilter;

#[derive(Default)]
pub struct Rules {
    matcher: Arc<SkimMatcherV2>,

    buffer: RwLock<Vec<Arc<Rule>>>,
    view: RwLock<Vec<Arc<Rule>>>,
}

impl Rules {
    pub fn push(&self, records: Vec<Rule>) {
        *self.buffer.write().unwrap() = records.into_iter().map(Arc::new).collect();
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
}

pub static RULE_COLS: &[ColDef<Rule>] = &[
    ColDef {
        id: "type",
        title: "Type",
        filterable: true,
        sortable: false,
        accessor: |c: &Rule| Cow::Borrowed(c.r#type.as_str()),
        sort_key: None,
    },
    ColDef {
        id: "payload",
        title: "Payload",
        filterable: true,
        sortable: false,
        accessor: |c: &Rule| crate::utils::rule_parser::format_payload(&c.r#type, &c.payload),
        sort_key: None,
    },
    ColDef {
        id: "proxy",
        title: "Proxy",
        filterable: true,
        sortable: false,
        accessor: |c: &Rule| Cow::Borrowed(c.proxy.as_str()),
        sort_key: None,
    },
];
