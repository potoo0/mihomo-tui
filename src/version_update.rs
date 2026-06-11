use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use semver::Version as SemverVersion;
use tracing::{debug, info};

use crate::api::{Api, GithubApi};

const RELEASE_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum VersionStatus {
    #[default]
    Unknown,
    Refreshing,
    UpToDate {
        current: String,
    },
    Available {
        current: String,
        latest: String,
    },
}

impl VersionStatus {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available { .. })
    }

    pub fn summary(&self) -> String {
        match self {
            Self::Unknown => "unknown".to_string(),
            Self::Refreshing => "refreshing...".to_string(),
            Self::UpToDate { current, .. } => format!("up to date ({current})"),
            Self::Available { current, latest } => format!("{current} -> {latest}"),
        }
    }
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct VersionUpdateState {
    pub app: VersionStatus,
    pub core: VersionStatus,
}

pub struct VersionUpdateAvailability {
    pub app: bool,
    pub core: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SharedVersionUpdateState(Arc<Mutex<VersionUpdateState>>);

impl SharedVersionUpdateState {
    pub fn lock(&self) -> MutexGuard<'_, VersionUpdateState> {
        self.0.lock().unwrap()
    }

    pub fn set_refreshing(&self) -> Option<VersionUpdateState> {
        let mut state = self.0.lock().unwrap();
        if matches!(state.app, VersionStatus::Refreshing)
            || matches!(state.core, VersionStatus::Refreshing)
        {
            return None;
        }
        let previous = state.clone();
        state.app = VersionStatus::Refreshing;
        state.core = VersionStatus::Refreshing;

        Some(previous)
    }

    pub async fn refresh(&self, api: &Api, mihomo_repo: &str) -> Result<()> {
        let Some(previous) = self.set_refreshing() else {
            debug!("version refresh is already in progress, skipping");
            return Ok(());
        };

        match refresh_version_status(api, mihomo_repo).await {
            Ok(next) => {
                *self.lock() = next;
                Ok(())
            }
            Err(e) => {
                *self.lock() = previous;
                Err(e)
            }
        }
    }

    pub fn is_available(&self) -> VersionUpdateAvailability {
        let state = self.0.lock().unwrap();
        VersionUpdateAvailability { app: state.app.is_available(), core: state.core.is_available() }
    }
}

async fn refresh_version_status(api: &Api, mihomo_repo: &str) -> Result<VersionUpdateState> {
    let github_api = GithubApi::new(RELEASE_CHECK_TIMEOUT)?;
    let app = refresh_app_version_status(&github_api).await?;
    let core = refresh_core_version_status(api, &github_api, mihomo_repo).await?;
    Ok(VersionUpdateState { app, core })
}

async fn refresh_app_version_status(github_api: &GithubApi) -> Result<VersionStatus> {
    let latest_tag = github_api
        .latest_release_tag(env!("CARGO_PKG_REPOSITORY"))
        .await?
        .ok_or_else(|| anyhow!("repository URL is not a GitHub repository"))?;

    let status = parse_version_status(env!("CARGO_PKG_VERSION"), &latest_tag)?;
    info!(?status, "app release version status refreshed");
    Ok(status)
}

async fn refresh_core_version_status(
    api: &Api,
    github_api: &GithubApi,
    mihomo_repo: &str,
) -> Result<VersionStatus> {
    let latest_tag = github_api
        .latest_release_tag(mihomo_repo)
        .await?
        .ok_or_else(|| anyhow!("mihomo repository is not a GitHub repository"))?;

    let current = api.get_version().await?;
    let status = parse_version_status(&current.version, &latest_tag)?;
    info!(?status, "core release version status refreshed");
    Ok(status)
}

pub fn parse_version_status(current: &str, latest_tag: &str) -> Result<VersionStatus> {
    let current = current.trim_start_matches('v');
    let latest = latest_tag.trim_start_matches('v');
    let current_version = SemverVersion::parse(current)?;
    let latest_version = SemverVersion::parse(latest)?;

    let current = current_version.to_string();
    let latest = latest_version.to_string();
    if latest_version > current_version {
        Ok(VersionStatus::Available { current, latest })
    } else {
        Ok(VersionStatus::UpToDate { current })
    }
}

pub fn update_app() -> Result<self_update::Status> {
    let (owner, repo) = github_owner_repo(env!("CARGO_PKG_REPOSITORY"))
        .ok_or_else(|| anyhow!("repository URL is not a GitHub repository"))?;

    let target = release_asset_target();
    self_update::backends::github::Update::configure()
        .repo_owner(owner)
        .repo_name(repo)
        .target(target)
        .bin_name(env!("CARGO_PKG_NAME"))
        .bin_path_in_archive(binary_name_in_archive())
        .show_download_progress(true)
        .show_output(true)
        .no_confirm(true)
        .current_version(env!("CARGO_PKG_VERSION"))
        .build()
        .context("failed to configure self updater")?
        .update()
        .context("failed to update app")
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartOutcome {
    Restarted,
    Unsupported,
}

pub fn restart_app(exe_path: &Path) -> Result<RestartOutcome> {
    #[cfg(unix)]
    {
        use std::env;
        use std::os::unix::process::CommandExt;
        use std::process::Command;

        let args = env::args_os().skip(1).filter(|arg| arg != "--update");
        let err = Command::new(exe_path).args(args).exec();
        return Err(anyhow!("failed to exec {}: {err}", exe_path.display()));
    }

    #[cfg(windows)]
    {
        // Do not auto-restart on Windows: spawning a new TUI process in the same console can
        // leave the child rendering but unable to receive keyboard input. Ask the user to restart.
        let _ = exe_path;
        return Ok(RestartOutcome::Unsupported);
    }

    #[allow(unreachable_code)]
    Ok(RestartOutcome::Restarted)
}

pub fn github_owner_repo(repository: &str) -> Option<(&str, &str)> {
    let trimmed = repository.trim().trim_end_matches(".git").trim_end_matches('/');
    let path = trimmed.strip_prefix("https://github.com/").unwrap_or(trimmed);
    let mut parts = path.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return None;
    }
    Some((owner, repo))
}

fn binary_name_in_archive() -> &'static str {
    if cfg!(windows) { "mihomo-tui.exe" } else { "mihomo-tui" }
}

pub fn release_asset_target() -> &'static str {
    match self_update::get_target() {
        "aarch64-apple-darwin" => "macOS-arm64",
        "x86_64-apple-darwin" => "macOS-x86_64",
        "x86_64-unknown-linux-gnu" | "x86_64-unknown-linux-musl" => "Linux-musl-x86_64",
        "aarch64-unknown-linux-gnu" | "aarch64-unknown-linux-musl" => "Linux-musl-arm64",
        "aarch64-linux-android" => "Linux-android-arm64",
        "x86_64-pc-windows-gnu" => "Windows-gnu-x86_64",
        "x86_64-pc-windows-msvc" => "Windows-msvc-x86_64",
        target => target,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_repository_url_parses_owner_and_repo() {
        assert_eq!(
            github_owner_repo("https://github.com/potoo0/mihomo-tui"),
            Some(("potoo0", "mihomo-tui"))
        );
        assert_eq!(
            github_owner_repo("https://github.com/potoo0/mihomo-tui.git"),
            Some(("potoo0", "mihomo-tui"))
        );
        assert_eq!(github_owner_repo("potoo0/mihomo-tui"), Some(("potoo0", "mihomo-tui")));
        assert_eq!(github_owner_repo("https://gitlab.com/potoo0/mihomo-tui"), None);
    }

    #[test]
    fn parse_version_status_classifies_release_tags() {
        assert!(parse_version_status("0.4.2", "v0.4.3").unwrap().is_available());
        assert!(!parse_version_status("0.4.2", "v0.4.2").unwrap().is_available());
    }

    #[test]
    fn set_refreshing_returns_previous_state_and_blocks_concurrent_refresh() {
        let state = SharedVersionUpdateState::default();
        let previous = VersionUpdateState {
            app: VersionStatus::UpToDate { current: "0.4.2".to_string() },
            core: VersionStatus::Available {
                current: "1.18.0".to_string(),
                latest: "1.19.0".to_string(),
            },
        };
        *state.lock() = previous.clone();

        assert_eq!(state.set_refreshing(), Some(previous));
        assert_eq!(
            *state.lock(),
            VersionUpdateState { app: VersionStatus::Refreshing, core: VersionStatus::Refreshing }
        );
        assert_eq!(state.set_refreshing(), None);
    }
}
