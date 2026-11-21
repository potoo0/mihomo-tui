use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use tracing::warn;

use crate::config::{Config, ConnectionsUiConfig, ProxyDetailUiConfig, ProxySetting, UiConfig};
use crate::store::connections::ConnectionsSetting;
use crate::store::hub::StoreHub;
use crate::store::proxy::{ProxyDetailSetting, ProxyProviderDetailSetting};

const SCHEMA_VERSION: u16 = 2;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RuntimeConfig {
    #[serde(rename = "$schema-version")]
    schema_version: u16,
    ui: Option<UiConfig>,
    proxy_setting: Option<ProxySetting>,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimePatch {
    pub language: Option<crate::i18n::Language>,
    pub connections: Option<ConnectionsUiConfig>,
    pub proxy_detail: Option<ProxyDetailUiConfig>,
    pub proxy_provider_detail: Option<ProxyDetailUiConfig>,
    pub proxy_setting: Option<ProxySetting>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self { schema_version: SCHEMA_VERSION, ui: None, proxy_setting: None }
    }
}

impl TryFrom<&StoreHub> for RuntimePatch {
    type Error = anyhow::Error;

    fn try_from(stores: &StoreHub) -> Result<Self> {
        Ok(Self {
            language: None,
            connections: stores
                .try_get::<ConnectionsSetting>()
                .map(|setting| ConnectionsUiConfig::try_from(&setting.snapshot()))
                .transpose()?,
            proxy_setting: stores.try_get::<ProxySetting>().map(|setting| setting.snapshot()),
            proxy_detail: stores
                .try_get::<ProxyDetailSetting>()
                .map(|setting| ProxyDetailUiConfig::from(setting.snapshot())),
            proxy_provider_detail: stores
                .try_get::<ProxyProviderDetailSetting>()
                .map(|setting| ProxyDetailUiConfig::from(setting.snapshot())),
        })
    }
}

impl RuntimeConfig {
    fn ensure_supported_schema(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION {
            bail!(
                "Unsupported runtime config schema version {}, expected {}",
                self.schema_version,
                SCHEMA_VERSION
            );
        }
        Ok(())
    }

    fn merge(&mut self, patch: RuntimePatch) {
        self.schema_version = SCHEMA_VERSION;

        if patch.language.is_some()
            || patch.connections.is_some()
            || patch.proxy_detail.is_some()
            || patch.proxy_provider_detail.is_some()
        {
            let ui = self.ui.get_or_insert_with(UiConfig::default);
            if let Some(language) = patch.language {
                ui.language = Some(language);
            }
            if let Some(connections) = patch.connections {
                ui.connections = Some(connections);
            }
            if let Some(proxy_detail) = patch.proxy_detail {
                ui.proxy_detail = Some(proxy_detail);
            }
            if let Some(proxy_provider_detail) = patch.proxy_provider_detail {
                ui.proxy_provider_detail = Some(proxy_provider_detail);
            }
        }

        if let Some(proxy_setting) = patch.proxy_setting {
            self.proxy_setting = Some(proxy_setting);
        }
    }

    fn apply_to(self, config: &mut Config) -> Result<()> {
        self.ensure_supported_schema()?;

        if let Some(runtime_ui) = self.ui {
            let ui = config.ui.get_or_insert_default();
            if let Some(language) = runtime_ui.language {
                ui.language = Some(language);
            }
            // Present sidecar blocks replace the corresponding main config blocks in full.
            if let Some(runtime_connections) = runtime_ui.connections {
                ui.connections = Some(runtime_connections);
            }
            if let Some(runtime_detail) = runtime_ui.proxy_detail {
                ui.proxy_detail = Some(runtime_detail);
            }
            if let Some(runtime_detail) = runtime_ui.proxy_provider_detail {
                ui.proxy_provider_detail = Some(runtime_detail);
            }
        }

        if let Some(runtime_proxy) = self.proxy_setting {
            config.proxy_setting = runtime_proxy;
        }

        Ok(())
    }
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
        warn!(
            error = ?err,
            path = %runtime_path.display(),
            "Failed to load runtime config; ignoring sidecar"
        );
    }
}

fn load_and_apply(config: &mut Config, runtime_path: &Path) -> Result<()> {
    let Some(runtime) = backup_incompatible_sidecar(runtime_path, load(runtime_path)?)? else {
        return Ok(());
    };

    let mut next = config.clone();
    runtime.apply_to(&mut next)?;
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

fn backup_incompatible_sidecar(
    runtime_path: &Path,
    runtime: Option<RuntimeConfig>,
) -> Result<Option<RuntimeConfig>> {
    let Some(runtime) = runtime else {
        return Ok(None);
    };
    let stored_schema_version = runtime.schema_version;
    if stored_schema_version == SCHEMA_VERSION {
        return Ok(Some(runtime));
    }

    let timestamp = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
    let mut backup_path = runtime_path.as_os_str().to_os_string();
    backup_path.push(format!(".v{stored_schema_version}.bak-{timestamp}"));
    let backup_path = PathBuf::from(backup_path);
    fs::rename(runtime_path, &backup_path).with_context(|| {
        format!(
            "Fail to back up incompatible runtime config `{}` to `{}`; preserving the original file",
            runtime_path.display(),
            backup_path.display()
        )
    })?;
    warn!(
        stored_schema_version,
        schema_version = SCHEMA_VERSION,
        path = %runtime_path.display(),
        backup_path = %backup_path.display(),
        "Runtime config schema mismatch; backed up the sidecar and reset runtime preferences to the main config and defaults"
    );
    Ok(None)
}

pub fn save(runtime_path: &Path, patch: RuntimePatch) -> Result<()> {
    let parent = runtime_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("Fail to create directory `{}`", parent.display()))?;

    // Unreadable sidecars can be rebuilt; backup failures must not fall through
    // to a write that would overwrite the existing preferences.
    let runtime = match load(runtime_path) {
        Ok(runtime) => runtime,
        Err(err) => {
            warn!(
                error = ?err,
                path = %runtime_path.display(),
                "Failed to load runtime config; rebuilding from defaults"
            );
            None
        }
    };
    let mut runtime = backup_incompatible_sidecar(runtime_path, runtime)?.unwrap_or_default();
    runtime.merge(patch);

    let raw = yaml_serde::to_string(&runtime).context("Fail to serialize runtime config")?;
    let mut temp = NamedTempFile::new_in(parent)
        .with_context(|| format!("Fail to create temporary file in `{}`", parent.display()))?;
    temp.write_all(raw.as_bytes()).with_context(|| {
        format!("Fail to write temporary runtime config in `{}`", parent.display())
    })?;
    temp.as_file().sync_all().context("Fail to sync temporary runtime config")?;
    temp.persist(runtime_path)
        .with_context(|| format!("Fail to replace runtime config `{}`", runtime_path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::num::NonZeroUsize;

    use super::*;
    use crate::config::{LatencyThreshold, ProxySetting, ProxySortConfig};
    use crate::models::sort::{ProxySortField, SortDir, SortSpec};
    use crate::store::connections::{ConnectionsSetting, DEFAULT_CONNECTION_COL_INDICES};
    use crate::store::query::QueryState;

    #[test]
    fn language_sidecar_is_optional_and_overrides_main_config() {
        let mut config = crate::config::default_config().unwrap();
        config.ui.as_mut().unwrap().language = Some(crate::i18n::Language::SimplifiedChinese);
        let old: RuntimeConfig = yaml_serde::from_str("$schema-version: 2").unwrap();
        old.apply_to(&mut config).unwrap();
        assert_eq!(
            config.ui.as_ref().unwrap().language,
            Some(crate::i18n::Language::SimplifiedChinese)
        );
        let runtime: RuntimeConfig =
            yaml_serde::from_str("$schema-version: 2\nui:\n  startup-tab: Logs").unwrap();
        runtime.apply_to(&mut config).unwrap();
        assert_eq!(
            config.ui.as_ref().unwrap().language,
            Some(crate::i18n::Language::SimplifiedChinese)
        );
        let runtime: RuntimeConfig =
            yaml_serde::from_str("$schema-version: 2\nui:\n  language: en").unwrap();
        let raw = yaml_serde::to_string(&runtime).unwrap();
        assert!(raw.contains("ui:\n  language: en\n"));
        let restored: RuntimeConfig = yaml_serde::from_str(&raw).unwrap();
        restored.apply_to(&mut config).unwrap();
        assert_eq!(config.ui.as_ref().unwrap().language, Some(crate::i18n::Language::English));
        config.ui = None;
        let runtime: RuntimeConfig = yaml_serde::from_str(&raw).unwrap();
        runtime.apply_to(&mut config).unwrap();
        assert_eq!(config.ui.as_ref().unwrap().language, Some(crate::i18n::Language::English));
    }

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
    fn startup_backs_up_incompatible_sidecars_and_uses_main_config() {
        for version in [1, 999] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("config.runtime.yaml");
            let legacy = format!(
                "# Previous preferences\n$schema-version: {version}\nproxy-setting:\n  test-url: https://legacy.example/test\n"
            );
            fs::write(&path, &legacy).unwrap();
            let mut config = crate::config::default_config().unwrap();
            config.proxy_setting.test_url = "https://main.example/test".into();

            try_load_and_apply(&mut config, &path);

            assert_eq!(config.proxy_setting.test_url, "https://main.example/test");
            assert!(!path.exists());
            let backups = fs::read_dir(dir.path())
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect::<Vec<_>>();
            assert_eq!(backups.len(), 1);
            let name = backups[0].file_name().unwrap().to_str().unwrap();
            let timestamp =
                name.strip_prefix(&format!("config.runtime.yaml.v{version}.bak-")).unwrap();
            assert!(timestamp.parse::<i128>().unwrap() > 0);
            assert_eq!(fs::read_to_string(&backups[0]).unwrap(), legacy);

            save(
                &path,
                RuntimePatch {
                    proxy_setting: Some(config.proxy_setting.clone()),
                    ..Default::default()
                },
            )
            .unwrap();
            let raw = fs::read_to_string(&path).unwrap();
            assert!(raw.contains("$schema-version: 2"));
            assert!(raw.contains("https://main.example/test"));
            try_load_and_apply(&mut config, &path);
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
            assert_eq!(fs::read_to_string(&backups[0]).unwrap(), legacy);
        }
    }

    #[test]
    fn save_backs_up_incompatible_sidecars_before_replacing_them() {
        for version in [1, 999] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("config.runtime.yaml");
            let legacy =
                format!("$schema-version: {version}\nui:\n  proxy-detail:\n    card-width: 40\n");
            fs::write(&path, &legacy).unwrap();

            save(&path, RuntimePatch::default()).unwrap();

            assert!(fs::read_to_string(&path).unwrap().contains("$schema-version: 2"));
            let backups = fs::read_dir(dir.path())
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|entry| entry != &path)
                .collect::<Vec<_>>();
            assert_eq!(backups.len(), 1);
            assert_eq!(fs::read_to_string(&backups[0]).unwrap(), legacy);
            assert!(!fs::read_to_string(&path).unwrap().contains("card-width: 40"));
        }
    }

    #[test]
    fn save_serializes_schema_version_and_snapshot() {
        let mut query_state = QueryState::new(DEFAULT_CONNECTION_COL_INDICES.len());
        query_state.sort = Some(SortSpec { col: 1, dir: crate::models::sort::SortDir::Desc });
        let setting = ConnectionsSetting {
            query_state,
            columns: DEFAULT_CONNECTION_COL_INDICES.to_vec().into(),
            column_widths: HashMap::from([(1, 24)]).into(),
            source_ip_alias: HashMap::from([("192.168.1.10".into(), "phone".into())]).into(),
        };
        let proxy = ProxySetting {
            test_url: "https://example.com/generate_204".into(),
            test_timeout: NonZeroUsize::new(3000).unwrap(),
            latency_threshold: LatencyThreshold { medium: 200, high: 800 },
            auto_terminate_connections: true,
        };
        let mut runtime = RuntimeConfig::default();
        runtime.merge(RuntimePatch {
            language: Some(crate::i18n::language()),
            connections: Some(ConnectionsUiConfig::try_from(&setting).unwrap()),
            proxy_setting: Some(proxy),
            proxy_detail: Some(ProxyDetailUiConfig {
                sort: Some(ProxySortConfig { field: ProxySortField::Name, dir: SortDir::Desc }),
                card_width: Some(40),
            }),
            proxy_provider_detail: Some(ProxyDetailUiConfig {
                sort: Some(ProxySortConfig { field: ProxySortField::Latency, dir: SortDir::Asc }),
                card_width: Some(32),
            }),
        });
        let raw = yaml_serde::to_string(&runtime).unwrap();

        assert!(raw.contains("$schema-version: 2"));
        let value: serde_json::Value = yaml_serde::from_str(&raw).unwrap();
        assert!(value.get("language").is_none());
        assert!(value["ui"]["language"].is_string());
        assert!(raw.contains("source-ip-alias:"));
        assert!(raw.contains("192.168.1.10: phone"));
        assert!(raw.contains("sort:"));
        assert!(raw.contains("field: Host"));
        assert!(raw.contains("dir: desc"));
        assert!(raw.contains("column-widths:"));
        assert!(raw.contains("Host: 24"));
        assert!(raw.contains("card-width: 40"));
        assert_eq!(value["ui"]["proxy-detail"]["sort"]["field"], "name");
        assert_eq!(value["ui"]["proxy-detail"]["sort"]["dir"], "desc");
        assert_eq!(value["ui"]["proxy-provider-detail"]["sort"]["field"], "latency");
        assert_eq!(value["ui"]["proxy-provider-detail"]["sort"]["dir"], "asc");
        assert_eq!(value["ui"]["proxy-provider-detail"]["card-width"], 32);
        assert!(!raw.contains("startup-tab:"));
        assert!(raw.contains("test-url: https://example.com/generate_204"));
        assert!(raw.contains("latency-threshold: 200,800"));
    }

    #[test]
    fn save_writes_runtime_file() {
        let runtime_path = crate::config::temp_config_path();
        let setting = ConnectionsSetting {
            query_state: QueryState::new(DEFAULT_CONNECTION_COL_INDICES.len()),
            columns: DEFAULT_CONNECTION_COL_INDICES.to_vec().into(),
            column_widths: Default::default(),
            source_ip_alias: Default::default(),
        };
        let mut proxy = ProxySetting::default();

        save(
            &runtime_path,
            RuntimePatch {
                language: Some(crate::i18n::language()),
                connections: Some(ConnectionsUiConfig::try_from(&setting).unwrap()),
                proxy_setting: Some(proxy.clone()),
                proxy_detail: Some(ProxyDetailUiConfig { sort: None, card_width: Some(40) }),
                proxy_provider_detail: Some(ProxyDetailUiConfig {
                    sort: None,
                    card_width: Some(32),
                }),
            },
        )
        .unwrap();
        proxy.test_url = "https://example.com/updated".into();
        save(&runtime_path, RuntimePatch { proxy_setting: Some(proxy), ..Default::default() })
            .unwrap();
        let raw = fs::read_to_string(&runtime_path).unwrap();
        fs::remove_file(&runtime_path).unwrap();

        assert!(raw.contains("$schema-version: 2"));
        assert!(raw.contains("proxy-setting:"));
        assert!(raw.contains("column-widths: {}"));
        assert!(raw.contains("source-ip-alias: {}"));
        assert!(raw.contains("card-width: 40"));
        assert!(raw.contains("proxy-provider-detail:\n    card-width: 32"));
        assert!(raw.contains("test-url: https://example.com/updated"));
        assert!(!raw.contains("startup-tab:"));
    }

    #[test]
    fn save_rebuilds_unreadable_sidecar() {
        let proxy = ProxySetting {
            test_url: "https://example.com/healed".into(),
            ..ProxySetting::default()
        };
        let patch = RuntimePatch { proxy_setting: Some(proxy.clone()), ..Default::default() };

        for raw in ["{ unmatched: [\n", "not a config\n"] {
            let runtime_path = crate::config::temp_config_path();
            fs::write(&runtime_path, raw).unwrap();

            save(&runtime_path, patch.clone()).unwrap();

            let healed = load(&runtime_path).unwrap().unwrap();
            fs::remove_file(&runtime_path).unwrap();
            assert_eq!(healed.schema_version, SCHEMA_VERSION);
            assert_eq!(healed.proxy_setting.unwrap().test_url, proxy.test_url);
        }
    }

    #[test]
    fn connections_snapshot_round_trip_clears_main_config_sort() {
        let mut config = crate::config::default_config().unwrap();
        config.ui = Some(
            yaml_serde::from_str(
                r#"connections:
  columns: [Host]
  sort: { field: Host, dir: asc }
  column-widths: { Host: 20 }
  source-ip-alias: { 192.168.1.10: phone }
"#,
            )
            .unwrap(),
        );
        RuntimeConfig::default().apply_to(&mut config).unwrap();
        assert!(config.ui.as_ref().unwrap().connections.as_ref().unwrap().sort.is_some());

        let stores = StoreHub::default();
        stores.register(ConnectionsSetting::default()).unwrap();
        let runtime_path = crate::config::temp_config_path();
        save(&runtime_path, RuntimePatch::try_from(&stores).unwrap()).unwrap();
        let raw = fs::read_to_string(&runtime_path).unwrap();
        let runtime = load(&runtime_path).unwrap().unwrap();
        fs::remove_file(&runtime_path).unwrap();
        assert!(raw.contains("columns:"));
        assert!(!raw.contains("sort:"));
        assert!(raw.contains("column-widths: {}"));
        assert!(raw.contains("source-ip-alias: {}"));
        runtime.apply_to(&mut config).unwrap();

        let connections = config.ui.as_ref().unwrap().connections.as_ref().unwrap();
        assert!(connections.sort.is_none());
        let setting = ConnectionsSetting::try_from(connections).unwrap();
        assert!(setting.query_state.sort.is_none());
        assert!(setting.column_widths.is_empty());
        assert!(setting.source_ip_alias.is_empty());
        assert_eq!(setting.columns, ConnectionsSetting::default().columns);
    }

    #[test]
    fn save_preserves_empty_detail_blocks() {
        let runtime_path = crate::config::temp_config_path();
        save(
            &runtime_path,
            RuntimePatch {
                language: Some(crate::i18n::language()),
                proxy_detail: Some(ProxyDetailUiConfig { sort: None, card_width: None }),
                proxy_provider_detail: Some(ProxyDetailUiConfig { sort: None, card_width: None }),
                ..Default::default()
            },
        )
        .unwrap();
        let raw = fs::read_to_string(&runtime_path).unwrap();
        fs::remove_file(&runtime_path).unwrap();

        assert!(raw.contains("language:"));
        assert!(raw.contains("proxy-detail: {}"));
        assert!(raw.contains("proxy-provider-detail: {}"));
    }

    #[test]
    fn empty_detail_blocks_round_trip_overrides_main_config() {
        let mut config = crate::config::default_config().unwrap();
        config.ui = Some(
            yaml_serde::from_str(
                r#"proxy-detail:
  sort:
    field: latency
  card-width: 40
proxy-provider-detail:
  sort:
    field: latency
  card-width: 32
"#,
            )
            .unwrap(),
        );
        for detail in [
            config.ui.as_ref().unwrap().proxy_detail.as_ref().unwrap(),
            config.ui.as_ref().unwrap().proxy_provider_detail.as_ref().unwrap(),
        ] {
            assert_eq!(detail.sort.as_ref().unwrap().field, ProxySortField::Latency);
        }

        let stores = StoreHub::default();
        stores.register(ProxyDetailSetting::default()).unwrap();
        stores.register(ProxyProviderDetailSetting::default()).unwrap();
        let runtime_path = crate::config::temp_config_path();
        save(&runtime_path, RuntimePatch::try_from(&stores).unwrap()).unwrap();
        let raw = fs::read_to_string(&runtime_path).unwrap();
        let runtime = load(&runtime_path).unwrap().unwrap();
        fs::remove_file(&runtime_path).unwrap();
        assert!(raw.contains("proxy-detail: {}"));
        assert!(raw.contains("proxy-provider-detail: {}"));
        runtime.apply_to(&mut config).unwrap();

        let ui = config.ui.as_ref().unwrap();
        let detail = ui.proxy_detail.as_ref().unwrap();
        assert!(detail.sort.is_none());
        assert!(detail.card_width.is_none());
        let setting = ProxyDetailSetting::from(detail);
        assert!(setting.sort.is_none());
        assert!(setting.card_width.is_none());

        let provider_detail = ui.proxy_provider_detail.as_ref().unwrap();
        assert!(provider_detail.sort.is_none());
        assert!(provider_detail.card_width.is_none());
        let setting = ProxyProviderDetailSetting::from(provider_detail);
        assert!(setting.sort.is_none());
        assert!(setting.card_width.is_none());
    }

    #[test]
    fn apply_rejects_unknown_schema_version() {
        let mut config = crate::config::default_config().unwrap();
        let err = RuntimeConfig { schema_version: 999, ui: None, proxy_setting: None }
            .apply_to(&mut config)
            .unwrap_err();

        assert!(err.to_string().contains("Unsupported runtime config schema version"));
    }
}
