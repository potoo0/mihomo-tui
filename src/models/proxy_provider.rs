use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::models::proxy::Proxy;

/// for [providerForApi](mihomo/adapter/provider/provider.go#providerForApi)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyProvider {
    pub name: String,
    pub vehicle_type: String,
    pub proxies: Vec<Proxy>,
    // pub test_url: String,
    pub subscription_info: Option<SubscriptionInfo>,

    /// updated time in RFC3339Nano format, e.g. "2006-01-02T15:04:05.999999999Z07:00"
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,

    // for ui only
    #[serde(skip)]
    pub updated_at_str: Option<Box<str>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SubscriptionInfo {
    pub download: Option<u64>,
    pub upload: Option<u64>,
    pub total: Option<u64>,
    /// expire time in unix timestamp, e.g. 1765256093
    pub expire: Option<u64>,
}
