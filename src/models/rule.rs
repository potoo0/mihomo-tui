use std::sync::atomic::AtomicBool;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Rule {
    pub r#type: String,
    pub payload: String,
    pub proxy: String,

    /// 0-based index of the rule in the list
    ///
    /// Available only when meta version >= v1.19.19
    pub index: Option<usize>,

    /// Extra runtime metadata of the rule
    ///
    /// Available only when meta version >= v1.19.19
    pub extra: Option<RuleExtra>,

    /// Number of sub-rules contained by this rule (e.g. GEOSITE, GEOIP); -1 if not applicable
    pub size: isize,

    // for ui only
    #[serde(skip)]
    pub disable_state: AtomicBool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleExtra {
    /// Whether this rule is disabled
    pub disabled: bool,
    /// Total number of times this rule has been matched
    pub hit_count: u64,
    /// Last hit time in RFC3339Nano format, e.g. "2006-01-02T15:04:05.999999999Z07:00"
    pub hit_at: Option<String>,
}

impl Rule {
    /// Whether the rule supports the `disabled` flag.
    #[inline]
    pub fn supports_disable(&self) -> bool {
        self.index.is_some() && self.extra.is_some()
    }
}
