use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::widgets::latency::Latency;

/// for [Proxy](mihomo/adapter/adapter.go#Proxy)
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proxy {
    pub name: String,
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,

    /// inner proxy groups or nodes, refers to [Proxy] name
    #[serde(rename = "all", skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<String>>,
    /// current selected node
    #[serde(rename = "now", skip_serializing_if = "Option::is_none")]
    pub selected: Option<String>,

    // pub test_url: Option<String>,
    /// delay history
    pub history: Vec<DelayHistory>,

    // for ui only
    #[serde(skip)]
    pub latency: Latency,

    /// API fields that are preserved for inspection but not used by the UI.
    #[serde(default, flatten)]
    pub unmodeled_fields: IndexMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DelayHistory {
    // /// time in RFC3339Nano format, e.g. "2006-01-02T15:04:05.999999999Z07:00"
    // pub time: String,
    /// delay in milliseconds, less than or equal to 0 means timeout
    pub delay: i64,

    /// API fields that are preserved for inspection but not used by the UI.
    #[serde(default, flatten)]
    pub unmodeled_fields: IndexMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn preserves_unmodeled_fields_with_api_names_and_order() {
        let proxy: Proxy = serde_json::from_value(json!({
            "name": "group",
            "type": "Selector",
            "all": ["alpha", "beta"],
            "now": "alpha",
            "history": [{
                "time": "2026-11-11T10:00:00Z",
                "delay": 42
            }],
            "udp": true,
            "xudp": false,
            "alive": true
        }))
        .unwrap();

        assert_eq!(
            proxy.unmodeled_fields.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["udp", "xudp", "alive"]
        );
        assert_eq!(
            proxy.history[0].unmodeled_fields.get("time").and_then(Value::as_str),
            Some("2026-11-11T10:00:00Z")
        );

        let value = serde_json::to_value(&proxy).unwrap();
        assert_eq!(value["all"], json!(["alpha", "beta"]));
        assert_eq!(value["now"], "alpha");
        assert_eq!(value["history"][0]["time"], "2026-11-11T10:00:00Z");
        assert!(value.get("latency").is_none());
        assert_eq!(
            value.as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["name", "type", "all", "now", "history", "udp", "xudp", "alive"]
        );
    }
}
