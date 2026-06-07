use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use anyhow::Result;

use crate::config::ConnectionsUiConfig;
use crate::models::sort::SortSpec;
use crate::store::connections::{DEFAULT_CONNECTION_COL_INDICES, with_alive_column};
use crate::store::query::QueryState;

pub static GLOBAL_CONNECTION_SETTING: OnceLock<RwLock<Arc<ConnectionsSetting>>> = OnceLock::new();

#[derive(Clone)]
pub struct ConnectionsSetting {
    pub query_state: QueryState,
    pub columns: Vec<usize>,
    pub source_ip_alias: HashMap<String, String>,
}

impl ConnectionsSetting {
    pub fn global() -> &'static RwLock<Arc<Self>> {
        GLOBAL_CONNECTION_SETTING.get_or_init(|| {
            let columns = DEFAULT_CONNECTION_COL_INDICES.to_vec();
            let setting = ConnectionsSetting {
                query_state: QueryState::new(columns.len()),
                columns,
                source_ip_alias: Default::default(),
            };

            RwLock::new(Arc::new(setting))
        })
    }

    pub fn snapshot() -> Arc<Self> {
        Arc::clone(&Self::global().read().unwrap())
    }

    pub fn update(f: impl FnOnce(&mut Self)) {
        let mut guard = Self::global().write().unwrap();
        let mut next = (**guard).clone();
        f(&mut next);
        *guard = Arc::new(next);
    }
}

impl TryFrom<&ConnectionsUiConfig> for ConnectionsSetting {
    type Error = anyhow::Error;

    fn try_from(value: &ConnectionsUiConfig) -> Result<Self> {
        let columns = with_alive_column(
            value
                .columns
                .as_deref()
                .map(ConnectionsUiConfig::parse_connections_columns)
                .transpose()?
                .unwrap_or_else(|| DEFAULT_CONNECTION_COL_INDICES.to_vec()),
        );
        let sort = value
            .sort
            .as_ref()
            .map(ConnectionsUiConfig::parse_connections_sort)
            .transpose()?
            .and_then(|sort| {
                columns
                    .iter()
                    .position(|&col| col == sort.col)
                    .map(|col| SortSpec { col, dir: sort.dir })
            });
        let query_state = QueryState { pattern: None, sort, max_cols: columns.len() };
        Ok(Self { columns, query_state, source_ip_alias: value.source_ip_alias.clone() })
    }
}
