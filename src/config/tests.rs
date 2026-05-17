use url::Url;

use super::*;
use crate::models::sort::{ProxySortField, SortDir, SortSpec};
use crate::store::connections::find_sortable_connection_col;

#[test]
fn test_config_default() {
    let default_config: Config = serde_yaml_ng::from_str(DEFAULT_CONFIG).unwrap();

    let config = load(None).unwrap();
    assert_eq!(config.mihomo_api, default_config.mihomo_api);
    assert_eq!(config.mihomo_secret, default_config.mihomo_secret);
    assert_eq!(config.log_file, default_config.log_file);
    assert_eq!(config.log_level, default_config.log_level);
    assert!(config.ui.is_some());
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
fn test_config_ui_connections_sort_only() {
    let cfg_path = TempFile::new(temp_config_path());

    let custom_config = r#"
mihomo-api: "http://localhost"
ui:
  connections:
    sort:
      field: "Host"
"#;
    fs::write(&cfg_path.0, custom_config).unwrap();

    let config = load(Some(cfg_path.0.clone())).unwrap();
    let ui = config.ui.as_ref().unwrap();
    let connections = ui.connections.as_ref().unwrap();
    let sort = connections.sort.as_ref().unwrap();

    assert_eq!(
        *sort,
        SortSpec { col: find_sortable_connection_col("Host").unwrap(), dir: SortDir::Desc }
    );
    assert!(ui.proxy_detail.is_none());

    drop(cfg_path);
}

#[test]
fn test_config_ui_connections_sort_case_insensitive() {
    let cfg_path = TempFile::new(temp_config_path());

    let custom_config = r#"
mihomo-api: "http://localhost"
ui:
  connections:
    sort:
      field: "hOsT"
      dir: asc
"#;
    fs::write(&cfg_path.0, custom_config).unwrap();

    let config = load(Some(cfg_path.0.clone())).unwrap();
    let ui = config.ui.as_ref().unwrap();
    let connections = ui.connections.as_ref().unwrap();

    assert_eq!(
        connections.sort,
        Some(SortSpec { col: find_sortable_connection_col("Host").unwrap(), dir: SortDir::Asc })
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

    assert!(connections.sort.is_none());
    assert!(ui.proxy_detail.is_none());

    drop(cfg_path);
}

#[test]
fn test_config_ui_connections_sort_invalid_field() {
    let cfg_path = TempFile::new(temp_config_path());

    let custom_config = r#"
mihomo-api: "http://localhost"
ui:
  connections:
    sort:
      field: "foo"
"#;
    fs::write(&cfg_path.0, custom_config).unwrap();

    let result = load(Some(cfg_path.0.clone()));
    assert!(result.is_err(), "expected error, got {:?}", result);

    let err_msg = format!("{:#}", result.unwrap_err());
    assert!(
        err_msg.contains("invalid `ui.connections.sort.field`"),
        "unexpected error: {}",
        err_msg
    );
    assert!(err_msg.contains("\"foo\""), "unexpected error: {}", err_msg);
    assert!(err_msg.contains("Host"), "unexpected error: {}", err_msg);

    drop(cfg_path);
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
