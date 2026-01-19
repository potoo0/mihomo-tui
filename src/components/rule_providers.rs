use std::borrow::Cow;
use std::string::ToString;
use std::sync::{Arc, RwLock};

use fuzzy_matcher::skim::SkimMatcherV2;
use indexmap::IndexMap;
use time::macros::format_description;

use crate::models::RuleProvider;
use crate::utils::columns::{ColDef, SortKey};
use crate::utils::row_filter::RowFilter;

const DATETIME_FMT: &[time::format_description::FormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

#[derive(Default)]
pub struct RuleProviders {
    matcher: Arc<SkimMatcherV2>,

    buffer: RwLock<Vec<Arc<RuleProvider>>>,
    view: RwLock<Vec<Arc<RuleProvider>>>,
}

impl RuleProviders {
    pub fn push(&self, records: IndexMap<String, RuleProvider>) {
        *self.buffer.write().unwrap() = records.into_values().map(Arc::new).collect();
    }

    pub fn compute_view(&self, pattern: Option<&str>) {
        let buffer = self.buffer.read().unwrap();

        let matcher = self.matcher.as_ref();
        let filtered = RowFilter::new(buffer.iter(), matcher, pattern, RULE_PROVIDER_COLS);
        let mut guard = self.view.write().unwrap();
        guard.clear();
        filtered.for_each(|v| guard.push(v));
    }

    pub fn with_view<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Vec<Arc<RuleProvider>>) -> R,
    {
        let guard = self.view.read().unwrap();
        f(&guard)
    }
}

pub static RULE_PROVIDER_COLS: &[ColDef<RuleProvider>] = &[
    ColDef {
        id: "name",
        title: "Name",
        filterable: true,
        sortable: true,
        accessor: |c: &RuleProvider| Cow::Borrowed(c.name.as_str()),
        sort_key: None,
    },
    ColDef {
        id: "vehicleType",
        title: "VehicleType",
        filterable: true,
        sortable: true,
        accessor: |c: &RuleProvider| Cow::Borrowed(c.vehicle_type.as_str()),
        sort_key: None,
    },
    ColDef {
        id: "behavior",
        title: "Behavior",
        filterable: true,
        sortable: true,
        accessor: |c: &RuleProvider| Cow::Borrowed(c.behavior.as_str()),
        sort_key: None,
    },
    ColDef {
        id: "rule_count",
        title: "RuleCount",
        filterable: false,
        sortable: true,
        accessor: |c: &RuleProvider| Cow::Owned(c.rule_count.to_string()),
        sort_key: None,
    },
    ColDef {
        id: "updated_at",
        title: "UpdatedAt",
        filterable: false,
        sortable: true,
        accessor: |c: &RuleProvider| {
            c.updated_at
                .and_then(|dt| dt.format(DATETIME_FMT).ok())
                .map(Cow::Owned)
                .unwrap_or(Cow::Borrowed("-"))
        },
        sort_key: Some(|c: &RuleProvider| {
            SortKey::U64(c.updated_at.map(|dt| dt.unix_timestamp() as u64).unwrap_or(0))
        }),
    },
];
