mod connections;

use std::collections::HashMap;

use color_eyre::eyre::{Context, eyre};
use color_eyre::{Result, eyre};
use futures::{Stream, StreamExt};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, header};
use strum::Display;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use tracing::debug;
use url::Url;

use crate::models::{Log, Version};

const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

pub struct Api {
    api: Url,
    bearer_token: Option<String>,
    client: Client,
}

#[derive(Debug, Clone, Copy, Display)]
pub enum LogLevel {
    #[strum(to_string = "error")]
    Error,
    #[strum(to_string = "warning")]
    Warning,
    #[strum(to_string = "info")]
    Info,
    #[strum(to_string = "debug")]
    Debug,
}

impl Api {
    pub fn new(api: Url, secret: Option<String>) -> Result<Api> {
        let client = Self::create_client(&secret)?;

        Ok(Self {
            api,
            bearer_token: secret,
            client,
        })
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
            .build()
            .context("Fail to build client")?;
        Ok(client)
    }

    pub async fn get_version(&self) -> Result<Version> {
        let body = self
            .client
            .get(self.api.join("/version")?)
            .send()
            .await
            .context("Fail to send `GET /version`")?
            .error_for_status()
            .context("Fail to request `GET /version`")?
            .json::<Version>()
            .await
            .context("Fail to parse response of `GET /version`")?;

        Ok(body)
    }

    pub async fn create_consumer(
        &self,
        path: &str,
        query_params: Option<HashMap<String, String>>,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>> {
        let mut url = self.api.clone().join(path)?;
        let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
        url.set_scheme(scheme)
            .map_err(|_| eyre!("Fail to set scheme"))?;
        if let Some(ref token) = self.bearer_token {
            url.query_pairs_mut().append_pair("token", token);
        }
        if let Some(params) = query_params {
            url.query_pairs_mut().extend_pairs(params);
        }
        let mut request = IntoClientRequest::into_client_request(&url)?;
        request
            .headers_mut()
            .insert(header::USER_AGENT, USER_AGENT.parse()?);
        debug!(
            "create_consumer, url: {}, headers: {:?}",
            url,
            request.headers()
        );
        let (stream, _) = connect_async(request).await?;
        Ok(stream)
    }

    pub async fn create_logs_consumer(
        &self,
        level: Option<LogLevel>,
    ) -> Result<impl Stream<Item = Result<Log>>> {
        let params = level.map(|l| HashMap::from([("level".to_string(), l.to_string())]));
        let stream = self.create_consumer("/logs", params).await?;
        let stream = stream.filter_map(|msg| async move {
            match msg {
                Ok(Message::Text(txt)) => match serde_json::from_str::<Log>(&txt) {
                    Ok(v) => Some(Ok(v)),
                    Err(e) => Some(Err(eyre::eyre!(e))),
                },
                _ => None,
            }
        });
        Ok(stream)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Once;

    use futures::{SinkExt, StreamExt, future};
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;
    use url::Url;

    use super::*;
    use crate::config::Config;

    fn init_logger() {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            let _ = tracing_subscriber::fmt()
                .with_writer(std::io::stderr)
                .with_max_level(tracing::Level::DEBUG)
                .try_init();
        });
    }

    #[tokio::test]
    async fn test_logs_consumer() {
        init_logger();
        let api = init_api();

        let token = CancellationToken::new();
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();

        let token_cloned = token.clone();
        tokio::task::Builder::new()
            .name("consumer")
            .spawn(async move {
                let stream = api
                    .create_logs_consumer(Some(LogLevel::Debug))
                    .await
                    .unwrap();
                let _ = stream
                    .take_until(token_cloned.cancelled())
                    .for_each(|msg| {
                        // let msg = msg.unwrap();
                        // let msg_text = msg.to_text().unwrap();
                        msg_tx.send(msg).unwrap();
                        future::ready(())
                    })
                    .await;
            })
            .unwrap();

        let mut cnt = 0;
        while let Some(msg) = msg_rx.recv().await {
            if cnt > 10 {
                token.cancel();
                break;
            }
            println!("msg: {msg:?}");
            cnt += 1;
        }
    }

    #[tokio::test]
    async fn test_get_version() {
        let api = init_api();
        let version = api.get_version().await;
        assert!(version.is_ok());
    }

    #[tokio::test]
    async fn test_get_version_fail() {
        let api = Api::new(
            Url::parse("http://localhost:19093").unwrap(),
            Some("1".to_string()),
        )
        .unwrap();
        let version = api.get_version().await;
        assert!(version.is_err());
    }

    fn init_api() -> Api {
        let config = Config::new(Some(PathBuf::from(
            "/home/wsl/.config/mihomo-tui/config.yaml",
        )))
        .unwrap();
        Api::new(config.mihomo_api, config.mihomo_secret).unwrap()
    }

    // todo remove
    #[test]
    fn test_tmp() {
        // let auth_value = HeaderValue::try_from(format!("Bearer {}", "token")).unwrap();
        // println!("is_sensitive: {}", auth_value.is_sensitive());

        let mut map = HeaderMap::new();

        map.insert("HOST", "hello".parse().unwrap());
        map.insert("HOST", "goodbye".parse().unwrap());
        map.insert("CONTENT_LENGTH", "123".parse().unwrap());
        map.append("CONTENT_LENGTH", "456".parse().unwrap());

        for (key, value) in map.iter() {
            println!("{:?}: {:?}", key, value);
        }
    }
}
