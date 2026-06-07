use std::num::NonZeroUsize;

use url::Url;

use super::*;
use crate::models::sort::{ProxySortField, SortDir, SortSpec};
use crate::store::connections::{CONNECTION_COLS, DEFAULT_CONNECTION_COL_INDICES};
use crate::store::connections_setting::ConnectionsSetting;

fn connection_col_index(title: &str) -> usize {
    CONNECTION_COLS
        .iter()
        .position(|def| def.title.eq_ignore_ascii_case(title))
        .unwrap_or_else(|| panic!("connection column {title:?} should exist"))
}

#[test]
fn test_config_default() {
    let default_config: Config = yaml_serde::from_str(DEFAULT_CONFIG).unwrap();

    let config = load(None).unwrap();
    assert_eq!(config.mihomo_api, default_config.mihomo_api);
    assert_eq!(config.mihomo_secret, default_config.mihomo_secret);
    assert_eq!(config.log_file, default_config.log_file);
    assert_eq!(config.log_level, default_config.log_level);
    assert!(config.ui.is_some());
    assert_eq!(
        config.ui.as_ref().unwrap().connections.as_ref().unwrap().columns,
        default_config.ui.as_ref().unwrap().connections.as_ref().unwrap().columns
    );
    assert_eq!(config.proxy_setting.test_url, default_config.proxy_setting.test_url);
    assert_eq!(config.proxy_setting.test_timeout, default_config.proxy_setting.test_timeout);
    assert_eq!(
        config.proxy_setting.latency_threshold,
        default_config.proxy_setting.latency_threshold
    );
    assert_eq!(
        config.proxy_setting.auto_terminate_connections,
        default_config.proxy_setting.auto_terminate_connections
    );
    assert_eq!(config.buffer.connections, default_config.buffer.connections);
    assert_eq!(config.buffer.logs, default_config.buffer.logs);
    assert_eq!(config.buffer.overview.memory, default_config.buffer.overview.memory);
    assert_eq!(config.buffer.overview.traffic, default_config.buffer.overview.traffic);
}

#[test]
fn test_config_existing_file() {
    let cfg_path = TempFile::new(temp_config_path());

    let custom_config = r#"
mihomo-api: "http://localhost"
mihomo-secret: "secret"
log-file: /tmp/log.log
log-level: "info"
"#;
    fs::write(&cfg_path.0, custom_config).unwrap();

    let config = load(Some(cfg_path.0.clone())).unwrap();
    assert_eq!(config.mihomo_api, Url::parse("http://localhost").unwrap());
    assert_eq!(config.mihomo_secret, Some("secret".to_owned()));
    assert_eq!(config.log_file, Some("/tmp/log.log".to_owned()));
    assert_eq!(config.log_level, Some("info".to_owned()));

    drop(cfg_path);
}

#[test]
fn test_config_proxy_setting_defaults_when_missing() {
    let cfg_path = TempFile::new(temp_config_path());

    let custom_config = r#"
mihomo-api: "http://localhost"
"#;
    fs::write(&cfg_path.0, custom_config).unwrap();

    let config = load(Some(cfg_path.0.clone())).unwrap();

    assert_eq!(config.proxy_setting.test_url, ProxySetting::default().test_url);
    assert_eq!(config.proxy_setting.test_timeout, ProxySetting::default().test_timeout);
    assert_eq!(config.proxy_setting.latency_threshold, LatencyThreshold::default());
    assert!(!config.proxy_setting.auto_terminate_connections);

    drop(cfg_path);
}

#[test]
fn test_config_proxy_setting_partial_defaults() {
    let cfg_path = TempFile::new(temp_config_path());

    let custom_config = r#"
mihomo-api: "http://localhost"
proxy-setting:
  test-timeout: 3000
  auto-terminate-connections: true
"#;
    fs::write(&cfg_path.0, custom_config).unwrap();

    let config = load(Some(cfg_path.0.clone())).unwrap();

    assert_eq!(config.proxy_setting.test_url, ProxySetting::default().test_url);
    assert_eq!(config.proxy_setting.test_timeout, NonZeroUsize::new(3000).unwrap());
    assert_eq!(config.proxy_setting.latency_threshold, LatencyThreshold::default());
    assert!(config.proxy_setting.auto_terminate_connections);

    drop(cfg_path);
}

#[test]
fn test_config_proxy_setting_custom() {
    let cfg_path = TempFile::new(temp_config_path());

    let custom_config = r#"
mihomo-api: "http://localhost"
proxy-setting:
  test-url: "https://example.com/generate_204"
  test-timeout: 3000
  latency-threshold: "300,800"
  auto-terminate-connections: true
"#;
    fs::write(&cfg_path.0, custom_config).unwrap();

    let config = load(Some(cfg_path.0.clone())).unwrap();

    assert_eq!(config.proxy_setting.test_url, "https://example.com/generate_204".to_owned());
    assert_eq!(config.proxy_setting.test_timeout, NonZeroUsize::new(3000).unwrap());
    assert_eq!(config.proxy_setting.latency_threshold, LatencyThreshold { medium: 300, high: 800 });
    assert!(config.proxy_setting.auto_terminate_connections);

    drop(cfg_path);
}

#[test]
fn test_config_proxy_setting_invalid_threshold() {
    let cfg_path = TempFile::new(temp_config_path());

    let custom_config = r#"
mihomo-api: "http://localhost"
proxy-setting:
  latency-threshold: "1000,500"
"#;
    fs::write(&cfg_path.0, custom_config).unwrap();

    let result = load(Some(cfg_path.0.clone()));
    assert!(result.is_err(), "expected error, got {:?}", result);

    let err_msg = format!("{:#}", result.unwrap_err());
    assert!(
        err_msg.contains("Threshold must satisfy medium < high"),
        "unexpected error: {}",
        err_msg
    );

    drop(cfg_path);
}

#[test]
fn test_config_proxy_setting_invalid_url() {
    let cfg_path = TempFile::new(temp_config_path());

    let custom_config = r#"
mihomo-api: "http://localhost"
proxy-setting:
  test-url: "ftp://example.com"
"#;
    fs::write(&cfg_path.0, custom_config).unwrap();

    let result = load(Some(cfg_path.0.clone()));
    assert!(result.is_err(), "expected error, got {:?}", result);

    let err_msg = format!("{:#}", result.unwrap_err());
    assert!(
        err_msg.contains("URL must start with http:// or https://"),
        "unexpected error: {}",
        err_msg
    );

    drop(cfg_path);
}

#[test]
fn test_config_proxy_setting_invalid_timeout() {
    let cfg_path = TempFile::new(temp_config_path());

    let custom_config = r#"
mihomo-api: "http://localhost"
proxy-setting:
  test-timeout: 0
"#;
    fs::write(&cfg_path.0, custom_config).unwrap();

    let result = load(Some(cfg_path.0.clone()));
    assert!(result.is_err(), "expected error, got {:?}", result);

    let err_msg = format!("{:#}", result.unwrap_err());
    assert!(
        err_msg.contains("invalid value: integer `0`, expected a nonzero usize"),
        "unexpected error: {}",
        err_msg
    );

    drop(cfg_path);
}

#[test]
fn test_config_proxy_setting_timeout_too_large() {
    let cfg_path = TempFile::new(temp_config_path());

    let custom_config = r#"
mihomo-api: "http://localhost"
proxy-setting:
  test-timeout: 60001
"#;
    fs::write(&cfg_path.0, custom_config).unwrap();

    let result = load(Some(cfg_path.0.clone()));
    assert!(result.is_err(), "expected error, got {:?}", result);

    let err_msg = format!("{:#}", result.unwrap_err());
    assert!(
        err_msg.contains("Timeout must be between 1 and 60000 milliseconds"),
        "unexpected error: {}",
        err_msg
    );

    drop(cfg_path);
}

#[test]
fn test_config_ser_error() {
    let cfg_path = TempFile::new(temp_config_path());

    let partial_config = r#"
mihomo-api: "localhost"
"#;
    fs::write(&cfg_path.0, partial_config).unwrap();

    let result = load(Some(cfg_path.0.clone()));
    assert!(result.is_err(), "expected error, got {:?}", result);

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Fail to deserialize file"),
        "expected contains `Fail to deserialize file`, but got {}",
        err_msg
    );

    drop(cfg_path);
}

#[test]
fn test_config_ui_connections_sort_uses_visible_column_index() {
    let cfg_path = TempFile::new(temp_config_path());

    let custom_config = r#"
mihomo-api: "http://localhost"
ui:
  connections:
    columns: ["Rule", "Host"]
    sort:
      field: "hOsT"
      dir: asc
"#;
    fs::write(&cfg_path.0, custom_config).unwrap();

    let config = load(Some(cfg_path.0.clone())).unwrap();
    let connections = config.ui.as_ref().unwrap().connections.as_ref().unwrap();
    let setting = ConnectionsSetting::try_from(connections).unwrap();

    assert_eq!(connections.columns, Some(vec!["Rule".to_owned(), "Host".to_owned()]));
    assert_eq!(connections.sort.as_ref().unwrap().field, "hOsT");

    assert_eq!(
        setting.columns,
        vec![
            connection_col_index("Alive"),
            connection_col_index("Rule"),
            connection_col_index("Host")
        ]
    );
    assert_eq!(setting.query_state.sort, Some(SortSpec { col: 2, dir: SortDir::Asc }));

    drop(cfg_path);
}

#[test]
fn test_config_ui_connections_columns_parse_case_insensitive() {
    let cfg_path = TempFile::new(temp_config_path());

    let custom_config = r#"
mihomo-api: "http://localhost"
ui:
  connections:
    columns: ["hOsT", "Rule", "downrate"]
"#;
    fs::write(&cfg_path.0, custom_config).unwrap();

    let config = load(Some(cfg_path.0.clone())).unwrap();
    let connections = config.ui.as_ref().unwrap().connections.as_ref().unwrap();
    let setting = ConnectionsSetting::try_from(connections).unwrap();

    assert_eq!(
        setting.columns,
        vec![
            connection_col_index("Alive"),
            connection_col_index("Host"),
            connection_col_index("Rule"),
            connection_col_index("DownRate"),
        ]
    );

    drop(cfg_path);
}

#[test]
fn test_config_ui_proxy_detail_sort_only() {
    let cfg_path = TempFile::new(temp_config_path());

    let custom_config = r#"
mihomo-api: "http://localhost"
ui:
  proxy-detail:
    sort:
      field: Latency
"#;
    fs::write(&cfg_path.0, custom_config).unwrap();

    let config = load(Some(cfg_path.0.clone())).unwrap();
    let ui = config.ui.as_ref().unwrap();
    let proxy_detail = ui.proxy_detail.as_ref().unwrap();
    let sort = proxy_detail.sort.as_ref().unwrap();

    assert_eq!(sort.field, ProxySortField::Latency);
    assert_eq!(sort.dir, SortDir::Asc);
    assert!(ui.connections.is_none());

    drop(cfg_path);
}

#[test]
fn test_config_ui_connections_without_sort() {
    let cfg_path = TempFile::new(temp_config_path());

    let custom_config = r#"
mihomo-api: "http://localhost"
ui:
  connections: {}
"#;
    fs::write(&cfg_path.0, custom_config).unwrap();

    let config = load(Some(cfg_path.0.clone())).unwrap();
    let ui = config.ui.as_ref().unwrap();
    let connections = ui.connections.as_ref().unwrap();
    let setting = ConnectionsSetting::try_from(connections).unwrap();

    assert!(connections.sort.is_none());
    assert!(connections.columns.is_none());
    assert_eq!(setting.columns, DEFAULT_CONNECTION_COL_INDICES);
    assert!(ui.proxy_detail.is_none());

    drop(cfg_path);
}

#[test]
fn test_config_ui_connections_columns_invalid_values() {
    let cases = [
        (r#"columns: ["Alive", "Host"]"#, "`ui.connections.columns` values must be one of"),
        (r#"columns: ["Host", "Foo"]"#, "`ui.connections.columns` values must be one of"),
        (r#"columns: ["Host", "host"]"#, "duplicate `ui.connections.columns` value"),
        (r#"columns: []"#, "`ui.connections.columns` cannot be empty, must be one of"),
    ];

    for (columns, expected_error) in cases {
        let cfg_path = TempFile::new(temp_config_path());
        let custom_config = format!(
            r#"
mihomo-api: "http://localhost"
ui:
  connections:
    {columns}
"#
        );
        fs::write(&cfg_path.0, custom_config).unwrap();

        let result = load(Some(cfg_path.0.clone()));
        assert!(result.is_err(), "expected error for {columns}, got {:?}", result);

        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(err_msg.contains(expected_error), "unexpected error: {}", err_msg);

        if columns.contains("Alive") {
            assert!(err_msg.contains("got \"Alive\""), "unexpected error: {}", err_msg);
            let allowed_values = err_msg
                .split("must be one of [")
                .nth(1)
                .and_then(|value| value.split(']').next())
                .unwrap_or_default();
            assert!(
                !allowed_values.contains("Alive"),
                "`Alive` should not be listed as an allowed configurable column: {}",
                err_msg
            );
        }
    }
}

#[test]
fn test_config_ui_connections_sort_hidden_by_columns_is_ignored() {
    let cfg_path = TempFile::new(temp_config_path());

    let custom_config = r#"
mihomo-api: "http://localhost"
ui:
  connections:
    columns: ["Host", "Rule"]
    sort:
      field: "DownRate"
"#;
    fs::write(&cfg_path.0, custom_config).unwrap();

    let config = load(Some(cfg_path.0.clone())).unwrap();
    let connections = config.ui.as_ref().unwrap().connections.as_ref().unwrap();
    let setting = ConnectionsSetting::try_from(connections).unwrap();

    assert!(setting.query_state.sort.is_none());

    drop(cfg_path);
}

#[test]
fn test_config_ui_connections_sort_invalid_field() {
    for field in ["foo", "Alive"] {
        let cfg_path = TempFile::new(temp_config_path());

        let custom_config = format!(
            r#"
mihomo-api: "http://localhost"
ui:
  connections:
    sort:
      field: "{field}"
"#
        );
        fs::write(&cfg_path.0, custom_config).unwrap();

        let result = load(Some(cfg_path.0.clone()));
        assert!(result.is_err(), "expected error for {field}, got {:?}", result);

        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("`ui.connections.sort.field` must be one of"),
            "unexpected error: {}",
            err_msg
        );
        assert!(err_msg.contains(&format!("got \"{field}\"")), "unexpected error: {}", err_msg);
        assert!(err_msg.contains("Host"), "unexpected error: {}", err_msg);

        if field == "Alive" {
            let allowed_values = err_msg
                .split("must be one of [")
                .nth(1)
                .and_then(|value| value.split(']').next())
                .unwrap_or_default();
            assert!(
                !allowed_values.contains("Alive"),
                "`Alive` should not be listed as an allowed sort field: {}",
                err_msg
            );
        }
    }
}

#[test]
fn test_config_buffer_defaults_when_missing() {
    let cfg_path = TempFile::new(temp_config_path());

    let custom_config = r#"
mihomo-api: "http://localhost"
"#;
    fs::write(&cfg_path.0, custom_config).unwrap();

    let config = load(Some(cfg_path.0.clone())).unwrap();

    assert_eq!(config.buffer.connections.get(), 500);
    assert_eq!(config.buffer.logs.get(), 500);
    assert_eq!(config.buffer.overview.memory.get(), 100);
    assert_eq!(config.buffer.overview.traffic.get(), 100);

    drop(cfg_path);
}

#[test]
fn test_config_buffer_overview_partial_defaults() {
    let cfg_path = TempFile::new(temp_config_path());
    let buffer_default = BufferConfig::default();

    let custom_config = r#"
mihomo-api: "http://localhost"
buffer:
  overview:
    memory: 200
"#;
    fs::write(&cfg_path.0, custom_config).unwrap();

    let config = load(Some(cfg_path.0.clone())).unwrap();

    assert_eq!(config.buffer.overview.memory.get(), 200);
    assert_eq!(config.buffer.overview.traffic, buffer_default.overview.traffic);
    assert_eq!(config.buffer.connections, buffer_default.connections);
    assert_eq!(config.buffer.logs, buffer_default.logs);

    drop(cfg_path);
}

struct TempFile(PathBuf);

impl TempFile {
    fn new(path: PathBuf) -> Self {
        Self(path)
    }

    fn remove(&self) {
        if self.0.is_file() {
            let _ = fs::remove_file(&self.0);
        }
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        self.remove();
    }
}
