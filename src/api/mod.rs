use anyhow::{Context, Result, anyhow};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, header};
use url::Url;

use crate::config::Config;

mod endpoints;
mod stream;
#[cfg(all(test, feature = "local-api-test"))]
mod tests;

const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

#[derive(Debug)]
pub struct Api {
    api: Url,
    bearer_token: Option<String>,
    client: Client,
}

impl Api {
    pub fn new(config: &Config) -> Result<Api> {
        let api = config.mihomo_api.clone();
        let secret = config.mihomo_secret.clone();
        let client = Self::create_client(&secret)?;

        Ok(Self { api, bearer_token: secret, client })
    }

    /// Create default headers for the API client.
    /// Currently, default_headers does not contain multiple values per key.
    fn default_headers(bearer_token: &Option<String>) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(header::USER_AGENT, USER_AGENT.parse()?);

        if let Some(token) = bearer_token {
            let mut auth_value = HeaderValue::try_from(format!("Bearer {}", token))?;
            auth_value.set_sensitive(true);
            headers.insert(header::AUTHORIZATION, auth_value);
        }
        Ok(headers)
    }

    fn create_client(bearer_token: &Option<String>) -> Result<Client> {
        let client = Client::builder()
            .default_headers(Self::default_headers(bearer_token)?)
            .no_proxy()
            .build()
            .context("Fail to build client")?;
        Ok(client)
    }

    async fn check_status(resp: reqwest::Response) -> Result<reqwest::Response> {
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }

        let url = resp.url().clone();
        let body = resp.text().await.unwrap_or_default();
        let mut msg = format!("HTTP status error ({}) for url ({})", status, url);

        if !body.is_empty() {
            msg.push_str("\nBody:");
            for line in body.lines() {
                msg.push_str(&format!("\n  {}", line));
            }
        }

        Err(anyhow!(msg))
    }
}
