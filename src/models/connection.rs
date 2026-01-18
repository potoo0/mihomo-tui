use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::utils::byte_size::ByteSize;

#[derive(Debug)]
pub struct ConnectionStats {
    pub conns_size: usize,
    pub memory: ByteSize,
    pub down_total: ByteSize,
    pub up_total: ByteSize,
}

impl From<&ConnectionsWrapper> for ConnectionStats {
    fn from(value: &ConnectionsWrapper) -> Self {
        ConnectionStats {
            conns_size: value.connections.as_ref().map(Vec::len).unwrap_or_default(),
            memory: value.memory.into(),
            down_total: value.download_total.into(),
            up_total: value.upload_total.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsWrapper {
    pub download_total: u64,
    pub upload_total: u64,
    pub connections: Option<Vec<Connection>>,
    pub memory: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub metadata: Value,
    pub upload: u64,
    pub download: u64,
    pub start: String,
    pub chains: Vec<String>,
    pub rule: String,
    pub rule_payload: String,

    // for ui only
    #[serde(skip)]
    pub inactive: Arc<AtomicBool>,
    #[serde(skip)]
    pub upload_rate: u64,
    #[serde(skip)]
    pub download_rate: u64,
}
