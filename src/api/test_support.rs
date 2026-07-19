use super::Api;
use crate::config::{MihomoApiEndpoint, default_config};

pub(super) fn test_api(endpoint: MihomoApiEndpoint, secret: Option<&str>) -> Api {
    let config = crate::config::Config {
        mihomo_api: endpoint,
        mihomo_secret: secret.map(str::to_owned),
        ..default_config().unwrap()
    };
    Api::new(&config).unwrap()
}
