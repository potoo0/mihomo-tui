use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::header::HeaderValue;
use reqwest::{Client, header};
use serde::Deserialize;

use super::USER_AGENT;

#[derive(Debug)]
pub struct GithubApi {
    client: Client,
}

impl GithubApi {
    pub fn new(timeout: Duration) -> Result<Self> {
        let default_headers =
            [(header::USER_AGENT, HeaderValue::from_static(USER_AGENT))].into_iter().collect();
        let client = Client::builder()
            .default_headers(default_headers)
            .timeout(timeout)
            .build()
            .context("Fail to build GitHub API client")?;
        Ok(Self { client })
    }

    pub async fn latest_release_tag(&self, repository: &str) -> Result<Option<String>> {
        #[derive(Debug, Deserialize)]
        struct GitHubRelease {
            tag_name: String,
        }

        let Some(url) = latest_release_api_url_from_repo(repository) else {
            return Ok(None);
        };

        let release = self
            .client
            .get(url)
            .send()
            .await
            .context("Fail to request latest GitHub release")?
            .error_for_status()
            .context("Fail to check latest GitHub release status")?
            .json::<GitHubRelease>()
            .await
            .context("Fail to parse latest GitHub release")?;
        Ok(Some(release.tag_name))
    }
}

fn latest_release_api_url_from_repo(repository: &str) -> Option<String> {
    let trimmed = repository.trim().trim_end_matches(".git").trim_end_matches('/');
    let path = trimmed.strip_prefix("https://github.com/").unwrap_or(trimmed);
    let mut parts = path.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return None;
    }
    Some(format!("https://api.github.com/repos/{owner}/{repo}/releases/latest"))
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn github_repository_maps_to_latest_release_api_url() {
        let cases = vec![
            (
                "https://github.com/potoo0/mihomo-tui",
                Some("https://api.github.com/repos/potoo0/mihomo-tui/releases/latest"),
            ),
            (
                "MetaCubeX/mihomo",
                Some("https://api.github.com/repos/MetaCubeX/mihomo/releases/latest"),
            ),
            (
                "https://github.com/potoo0/mihomo-tui.git",
                Some("https://api.github.com/repos/potoo0/mihomo-tui/releases/latest"),
            ),
            ("https://gitlab.com/potoo0/mihomo-tui", None),
        ];

        for (repository, expected) in cases {
            assert_eq!(latest_release_api_url_from_repo(repository), expected.map(str::to_owned));
        }
    }

    #[cfg(feature = "local-api-test")]
    #[tokio::test]
    async fn latest_release_tag_returns_tag_from_github_repository() {
        use semver::Version;
        use tracing::info;

        use crate::utils::test::init_logger;

        init_logger();
        let api = GithubApi::new(Duration::from_secs(10)).unwrap();

        let repository = "cli/cli";
        let tag = api.latest_release_tag(repository).await.unwrap().unwrap();
        let version = Version::parse(tag.trim_start_matches('v')).unwrap();
        info!(repository, tag, parsed_version = ?version, "Fetched latest GitHub release tag");

        assert!(version.major > 1);
    }
}
