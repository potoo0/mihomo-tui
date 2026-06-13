use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::config::{Config, ConnectionsUiConfig, ProxySetting, UiConfig};
use crate::store::connections_setting::ConnectionsSetting;

const SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RuntimeConfig {
    #[serde(rename = "$schema-version")]
    schema_version: u16,
    ui: Option<UiConfig>,
    proxy_setting: Option<ProxySetting>,
}

impl RuntimeConfig {
    fn new(connections: &ConnectionsSetting, proxy_setting: &ProxySetting) -> Result<Self> {
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            ui: Some(UiConfig {
                connections: Some(ConnectionsUiConfig::try_from(connections)?),
                proxy_detail: None,
                proxy_provider_detail: None,
            }),
            proxy_setting: Some(proxy_setting.clone()),
        })
    }
}

fn is_empty_connections(connections: &ConnectionsUiConfig) -> bool {
    connections.columns.is_none()
        && connections.sort.is_none()
        && connections.source_ip_alias.is_empty()
}

pub fn runtime_path_for(config_path: &Path) -> PathBuf {
    let Some(file_name) = config_path.file_name().and_then(|name| name.to_str()) else {
        return config_path.with_extension("runtime.yaml");
    };

    let runtime_file_name = match (config_path.file_stem(), config_path.extension()) {
        (Some(stem), Some(ext)) => {
            format!("{}.runtime.{}", stem.to_string_lossy(), ext.to_string_lossy())
        }
        _ => format!("{file_name}.runtime.yaml"),
    };
    config_path.with_file_name(runtime_file_name)
}

pub fn try_load_and_apply(config: &mut Config, runtime_path: &Path) {
    if let Err(err) = load_and_apply(config, runtime_path) {
        error!(
            error = ?err,
            path = %runtime_path.display(),
            "Failed to load runtime config; ignoring sidecar"
        );
    }
}

fn load_and_apply(config: &mut Config, runtime_path: &Path) -> Result<()> {
    let Some(runtime) = load(runtime_path)? else {
        return Ok(());
    };

    let mut next = config.clone();
    apply(&mut next, runtime)?;
    next.validate()?;
    *config = next;
    Ok(())
}

pub fn load(runtime_path: &Path) -> Result<Option<RuntimeConfig>> {
    if !runtime_path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(runtime_path)
        .with_context(|| format!("Fail to read runtime config `{}`", runtime_path.display()))?;
    let runtime: RuntimeConfig = yaml_serde::from_str(&raw).with_context(|| {
        format!("Fail to deserialize runtime config `{}`", runtime_path.display())
    })?;
    Ok(Some(runtime))
}

fn apply(config: &mut Config, runtime: RuntimeConfig) -> Result<()> {
    if runtime.schema_version != SCHEMA_VERSION {
        bail!(
            "Unsupported runtime config schema version {}, expected {}",
            runtime.schema_version,
            SCHEMA_VERSION
        );
    }

    if let Some(runtime_connections) = runtime.ui.and_then(|ui| ui.connections)
        && !is_empty_connections(&runtime_connections)
    {
        let ui = config.ui.get_or_insert(UiConfig {
            connections: None,
            proxy_detail: None,
            proxy_provider_detail: None,
        });
        ui.connections = Some(runtime_connections);
    }

    if let Some(runtime_proxy) = runtime.proxy_setting {
        config.proxy_setting = runtime_proxy;
    }

    Ok(())
}

pub fn save(
    runtime_path: &Path,
    connections: &ConnectionsSetting,
    proxy_setting: &ProxySetting,
) -> Result<()> {
    if let Some(parent) = runtime_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Fail to create directory `{}`", parent.display()))?;
    }

    let runtime = RuntimeConfig::new(connections, proxy_setting)?;
    let raw = yaml_serde::to_string(&runtime).context("Fail to serialize runtime config")?;
    fs::write(runtime_path, raw)
        .with_context(|| format!("Fail to write runtime config `{}`", runtime_path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::num::NonZeroUsize;

    use super::*;
    use crate::config::{LatencyThreshold, ProxySetting};
    use crate::models::sort::SortSpec;
    use crate::store::connections::DEFAULT_CONNECTION_COL_INDICES;
    use crate::store::query::QueryState;

    #[test]
    fn runtime_path_for_config_with_yaml_extension() {
        assert_eq!(
            runtime_path_for(Path::new("/tmp/config.yaml")),
            PathBuf::from("/tmp/config.runtime.yaml")
        );
        assert_eq!(
            runtime_path_for(Path::new("/tmp/config.yml")),
            PathBuf::from("/tmp/config.runtime.yml")
        );
        assert_eq!(
            runtime_path_for(Path::new("/tmp/config")),
            PathBuf::from("/tmp/config.runtime.yaml")
        );
    }

    #[test]
    fn save_serializes_schema_version_and_snapshot() {
        let mut query_state = QueryState::new(DEFAULT_CONNECTION_COL_INDICES.len());
        query_state.sort = Some(SortSpec { col: 1, dir: crate::models::sort::SortDir::Desc });
        let setting = ConnectionsSetting {
            query_state,
            columns: DEFAULT_CONNECTION_COL_INDICES.to_vec(),
            source_ip_alias: HashMap::from([("192.168.1.10".into(), "phone".into())]),
        };
        let proxy = ProxySetting {
            test_url: "https://example.com/generate_204".into(),
            test_timeout: NonZeroUsize::new(3000).unwrap(),
            latency_threshold: LatencyThreshold { medium: 200, high: 800 },
            auto_terminate_connections: true,
        };
        let runtime = RuntimeConfig::new(&setting, &proxy).unwrap();
        let raw = yaml_serde::to_string(&runtime).unwrap();

        assert!(raw.contains("$schema-version: 1"));
        assert!(raw.contains("source-ip-alias:"));
        assert!(raw.contains("192.168.1.10: phone"));
        assert!(raw.contains("sort:"));
        assert!(raw.contains("field: Host"));
        assert!(raw.contains("dir: desc"));
        assert!(raw.contains("test-url: https://example.com/generate_204"));
        assert!(raw.contains("latency-threshold: 200,800"));
    }

    #[test]
    fn save_writes_runtime_file() {
        let runtime_path = crate::config::temp_config_path();
        let setting = ConnectionsSetting {
            query_state: QueryState::new(DEFAULT_CONNECTION_COL_INDICES.len()),
            columns: DEFAULT_CONNECTION_COL_INDICES.to_vec(),
            source_ip_alias: HashMap::new(),
        };
        let proxy = ProxySetting::default();

        save(&runtime_path, &setting, &proxy).unwrap();
        let raw = fs::read_to_string(&runtime_path).unwrap();
        fs::remove_file(&runtime_path).unwrap();

        assert!(raw.contains("$schema-version: 1"));
        assert!(raw.contains("proxy-setting:"));
    }

    #[test]
    fn apply_rejects_unknown_schema_version() {
        let mut config = crate::config::default_config().unwrap();
        let err =
            apply(&mut config, RuntimeConfig { schema_version: 2, ui: None, proxy_setting: None })
                .unwrap_err();

        assert!(err.to_string().contains("Unsupported runtime config schema version"));
    }
}
