pub mod connections;
pub mod logs;
pub mod proxies;
pub mod proxy_providers;
pub mod proxy_setting;
pub mod query;
pub mod rule_providers;
pub mod rules;

pub const BUFFER_SIZE: usize = 100;
pub const CONNS_BUFFER_SIZE: usize = 500;
pub const LOGS_BUFFER_SIZE: usize = 500;
