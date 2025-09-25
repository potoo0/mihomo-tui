use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::models::proxy::Proxy;

#[derive(Debug, Clone, Deserialize)]
pub struct ProxyProvidersWrapper {
    pub providers: HashMap<String, ProxyProvider>,
}

/// for [providerForApi](mihomo/adapter/provider/provider.go#providerForApi)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyProvider {
    pub name: String,
    pub vehicle_type: String,
    pub proxies: Vec<Proxy>,
    pub test_url: String,
    /// updated time in RFC3339Nano format, e.g. "2006-01-02T15:04:05.999999999Z07:00"
    pub updated_at: Option<String>,
    pub subscription_info: Option<SubscriptionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionInfo {
    pub download: Option<u64>,
    pub upload: Option<u64>,
    pub total: Option<u64>,
    /// expire time in unix timestamp, e.g. 1765256093
    pub expire: Option<u64>,
}
