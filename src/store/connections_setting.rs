use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use anyhow::{Result, anyhow};

use crate::config::{ConnectionsSortConfig, ConnectionsUiConfig};
use crate::models::sort::SortSpec;
use crate::store::connections::{
    ALIVE_COLUMN_INDEX, CONNECTION_COLS, DEFAULT_CONNECTION_COL_INDICES, with_alive_column,
};
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

impl TryFrom<&ConnectionsSetting> for ConnectionsUiConfig {
    type Error = anyhow::Error;

    fn try_from(value: &ConnectionsSetting) -> Result<Self> {
        let columns = value
            .columns
            .iter()
            .copied()
            .filter(|&idx| idx != ALIVE_COLUMN_INDEX)
            .map(|idx| {
                CONNECTION_COLS
                    .get(idx)
                    .map(|def| def.col.title.to_owned())
                    .ok_or_else(|| anyhow!("connection column index {idx} does not exist"))
            })
            .collect::<Result<Vec<_>>>()?;

        let sort = match value.query_state.sort {
            None => None,
            Some(sort) => {
                let runtime_col =
                    value.columns.get(sort.col).cloned().ok_or_else(|| {
                        anyhow!("connection sort column {} does not exist", sort.col)
                    })?;
                if runtime_col == ALIVE_COLUMN_INDEX {
                    None
                } else {
                    let field = CONNECTION_COLS
                        .get(runtime_col)
                        .map(|def| def.col.title.to_owned())
                        .ok_or_else(|| {
                            anyhow!("connection column index {runtime_col} does not exist")
                        })?;
                    Some(ConnectionsSortConfig { field, dir: sort.dir })
                }
            }
        };

        Ok(ConnectionsUiConfig {
            columns: Some(columns),
            sort,
            source_ip_alias: value.source_ip_alias.clone(),
        })
    }
}
