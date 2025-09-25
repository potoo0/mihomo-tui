use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ProxiesWrapper {
    pub proxies: HashMap<String, Proxy>,
}

/// for [Proxy](mihomo/adapter/adapter.go#Proxy)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Proxy {
    pub name: String,
    pub r#type: String,
    pub hidden: Option<bool>,

    /// inner proxy groups or nodes, refers to [Proxy] name
    pub all: Option<Vec<String>>,
    /// current selected node
    #[serde(rename(deserialize = "now"))]
    pub selected: Option<String>,

    pub test_url: Option<String>,
    /// delay history
    pub history: Vec<DelayHistory>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DelayHistory {
    /// time in RFC3339Nano format, e.g. "2006-01-02T15:04:05.999999999Z07:00"
    pub time: String,
    /// delay in milliseconds, less than or equal to 0 means timeout
    pub delay: i64,
}
