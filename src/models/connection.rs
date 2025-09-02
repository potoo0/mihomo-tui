use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::utils::byte_size::ByteSize;

#[derive(Debug, Clone)]
pub struct ConnectionStat {
    pub conns_size: usize,
    pub memory: ByteSize,
    pub down_total: ByteSize,
    pub up_total: ByteSize,
}

impl From<&ConnectionWrapper> for ConnectionStat {
    fn from(value: &ConnectionWrapper) -> Self {
        ConnectionStat {
            conns_size: value.connections.len(),
            memory: value.memory.into(),
            down_total: value.download_total.into(),
            up_total: value.upload_total.into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionWrapper {
    pub download_total: u64,
    pub upload_total: u64,
    pub connections: Vec<Connection>,
    pub memory: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

// #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
// #[serde(rename_all = "camelCase")]
// pub struct ConnectionMetadata {
//     pub network: String,
//     pub r#type: String,
//     #[serde(rename(deserialize = "sourceIP"))]
//     pub source_ip: String,
//     #[serde(rename(deserialize = "destinationIP"))]
//     pub destination_ip: String,
//     #[serde(rename(deserialize = "sourceGeoIP"))]
//     pub source_geo_ip: Option<String>,
//     #[serde(rename(deserialize = "destinationGeoIP"))]
//     pub destination_geo_ip: Option<String>,
//     #[serde(rename(deserialize = "sourceIPASN"))]
//     pub source_ip_asn: String,
//     #[serde(rename(deserialize = "destinationIPASN"))]
//     pub destination_ip_asn: String,
//     pub source_port: String,
//     pub destination_port: String,
//     #[serde(rename(deserialize = "inboundIP"))]
//     pub inbound_ip: String,
//     pub inbound_port: String,
//     pub inbound_name: String,
//     pub inbound_user: String,
//     pub host: String,
//     pub dns_mode: String,
//     pub uid: u32,
//     pub process: String,
//     pub process_path: String,
//     pub special_proxy: String,
//     pub special_rules: String,
//     pub remote_destination: String,
//     #[serde(rename(deserialize = "dscp"))]
//     pub dscp: u32,
//     pub sniff_host: String,
// }
