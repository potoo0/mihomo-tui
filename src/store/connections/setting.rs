use std::collections::{BTreeMap, HashMap};
use std::num::NonZeroU16;
use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::config::{ConnectionsSortConfig, ConnectionsUiConfig};
use crate::models::sort::SortSpec;
use crate::store::connections::{
    ALIVE_COLUMN_INDEX, CONNECTION_COLS, DEFAULT_CONNECTION_COL_INDICES, with_alive_column,
};
use crate::store::hub::StoreLifecycle;
use crate::store::query::QueryState;

/// Runtime settings for the connections table.
///
/// A snapshot is cloned on every render, so the heap-backed fields are shared
/// through [`Arc`]; mutate them in place with [`Arc::make_mut`].
#[derive(Clone)]
pub struct ConnectionsSetting {
    /// Runtime filtering and sorting state.
    ///
    /// `query_state.sort.col` is a position in [`Self::columns`],
    /// not an index into [`CONNECTION_COLS`].
    pub query_state: QueryState,

    /// Ordered connection columns displayed at runtime.
    ///
    /// Each value is a stable index into [`CONNECTION_COLS`], while its position in this vector
    /// is the column's visible index. The runtime-only Alive column is included here even though
    /// it is not stored in user configuration.
    pub columns: Arc<[usize]>,

    /// User-defined column widths keyed by stable indices into [`CONNECTION_COLS`].
    pub column_widths: Arc<HashMap<usize, u16>>,

    /// Display aliases keyed by source IP address.
    pub source_ip_alias: Arc<HashMap<String, String>>,
}

impl Default for ConnectionsSetting {
    fn default() -> Self {
        let columns = DEFAULT_CONNECTION_COL_INDICES.to_vec();
        Self {
            query_state: QueryState::new(columns.len()),
            columns: columns.into(),
            column_widths: Default::default(),
            source_ip_alias: Default::default(),
        }
    }
}

impl StoreLifecycle for ConnectionsSetting {}

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
        let column_widths = match &value.column_widths {
            Some(widths) => ConnectionsUiConfig::parse_connections_column_widths(widths)?,
            None => HashMap::new(),
        };
        let source_ip_alias = value
            .source_ip_alias
            .iter()
            .flatten()
            .map(|(source_ip, alias)| (source_ip.clone(), alias.clone()))
            .collect::<HashMap<_, _>>();
        Ok(Self {
            columns: columns.into(),
            query_state,
            column_widths: column_widths.into(),
            source_ip_alias: source_ip_alias.into(),
        })
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

        let column_widths = value
            .column_widths
            .iter()
            .map(|(&idx, &width)| {
                let title = CONNECTION_COLS
                    .get(idx)
                    .filter(|_| idx != ALIVE_COLUMN_INDEX)
                    .map(|def| def.col.title.to_owned())
                    .ok_or_else(|| anyhow!("connection column index {idx} does not exist"))?;
                let width = NonZeroU16::new(width)
                    .ok_or_else(|| anyhow!("connection column width must be greater than zero"))?;
                Ok((title, width))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;

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
        let source_ip_alias = Some(
            value
                .source_ip_alias
                .iter()
                .map(|(source_ip, alias)| (source_ip.clone(), alias.clone()))
                .collect(),
        );

        Ok(ConnectionsUiConfig {
            columns: Some(columns),
            sort,
            column_widths: Some(column_widths),
            source_ip_alias,
        })
    }
}
