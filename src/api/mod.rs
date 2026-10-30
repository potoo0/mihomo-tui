use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, ClientBuilder, header};
use tracing::debug;
use url::Url;

use crate::config::{Config, MihomoApiEndpoint};

mod endpoints;
mod github;
#[cfg(all(test, feature = "local-api-test"))]
mod local_api_tests;
mod stream;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

pub use github::GithubApi;

const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub struct Api {
    api: Url,
    endpoint: MihomoApiEndpoint,
    bearer_token: Option<String>,
    client: Client,
}

impl Api {
    pub fn new(config: &Config) -> Result<Api> {
        let endpoint = config.mihomo_api.clone();
        let api = match &endpoint {
            MihomoApiEndpoint::Http(url) => url.clone(),
            MihomoApiEndpoint::UnixSocket(_) | MihomoApiEndpoint::WindowsNamedPipe(_) => {
                Url::parse("http://localhost").expect("static IPC base URL must be valid")
            }
        };
        let bearer_token = match &endpoint {
            MihomoApiEndpoint::Http(_) => config.mihomo_secret.clone(),
            MihomoApiEndpoint::UnixSocket(_) | MihomoApiEndpoint::WindowsNamedPipe(_) => {
                if config.mihomo_secret.is_some() {
                    debug!("mihomo-secret is ignored for IPC API transport");
                }
                None
            }
        };
        let client = Self::create_client(&endpoint, &bearer_token)?;

        Ok(Self { api, endpoint, bearer_token, client })
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

    fn create_client(
        endpoint: &MihomoApiEndpoint,
        bearer_token: &Option<String>,
    ) -> Result<Client> {
        let builder = configure_tls_roots(
            Client::builder()
                .default_headers(Self::default_headers(bearer_token)?)
                .no_proxy()
                .connect_timeout(CONNECT_TIMEOUT)
                .timeout(REQUEST_TIMEOUT),
        )?;
        let builder = match endpoint {
            MihomoApiEndpoint::Http(_) => builder,
            MihomoApiEndpoint::UnixSocket(path) => {
                #[cfg(unix)]
                {
                    builder.unix_socket(path.as_path())
                }
                #[cfg(not(unix))]
                anyhow::bail!(
                    "Unix socket mihomo API `{}` is not supported on this platform",
                    path.display()
                )
            }
            MihomoApiEndpoint::WindowsNamedPipe(pipe) => {
                #[cfg(windows)]
                {
                    builder.windows_named_pipe(pipe.as_str())
                }
                #[cfg(not(windows))]
                anyhow::bail!(
                    "Windows named pipe mihomo API `{pipe}` is not supported on this platform"
                )
            }
        };
        let client = builder.build().context("Fail to build client")?;
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

#[cfg(target_os = "android")]
pub(crate) fn webpki_root_certificates() -> Result<Vec<reqwest::Certificate>> {
    webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .map(|cert| reqwest::Certificate::from_der(cert.as_ref()))
        .collect::<reqwest::Result<Vec<_>>>()
        .context("Fail to load WebPKI root certificates")
}

#[cfg(target_os = "android")]
fn configure_tls_roots(builder: ClientBuilder) -> Result<ClientBuilder> {
    let roots = webpki_root_certificates()?;

    Ok(builder.tls_certs_only(roots))
}

#[cfg(not(target_os = "android"))]
fn configure_tls_roots(builder: ClientBuilder) -> Result<ClientBuilder> {
    Ok(builder)
}
