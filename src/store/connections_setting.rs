use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use crate::config::ConnectionsUiConfig;
use crate::store::connections::DEFAULT_CONNECTION_COL_INDICES;
use crate::store::query::QueryState;

pub static GLOBAL_CONNECTION_SETTING: OnceLock<RwLock<Arc<ConnectionsSetting>>> = OnceLock::new();

#[derive(Clone)]
pub struct ConnectionsSetting {
    // TODO reset or migrate `sort` if columns changed.
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

impl From<&ConnectionsUiConfig> for ConnectionsSetting {
    fn from(config: &ConnectionsUiConfig) -> Self {
        let query_state =
            QueryState { pattern: None, sort: config.sort, max_cols: config.columns.len() };
        Self {
            columns: config.columns.clone(),
            query_state,
            source_ip_alias: config.source_ip_alias.clone(),
        }
    }
}
