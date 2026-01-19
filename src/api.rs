use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use futures_util::{Stream, StreamExt};
use indexmap::IndexMap;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use reqwest::{Client, header};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tracing::debug;
use url::Url;

use crate::config::Config;
use crate::models::proxy::Proxy;
use crate::models::proxy_provider::ProxyProvider;
use crate::models::{
    ConnectionsWrapper, CoreConfig, Log, LogLevel, Memory, Rule, RuleProvider, Traffic, Version,
};

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

    pub async fn create_stream<T>(
        &self,
        path: &str,
        query_params: Option<HashMap<String, String>>,
    ) -> Result<impl Stream<Item = Result<T>>>
    where
        T: DeserializeOwned,
    {
        let mut url = self.api.clone().join(path)?;
        let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
        url.set_scheme(scheme).map_err(|_| anyhow!("Fail to set scheme"))?;
        // append query params
        if let Some(ref token) = self.bearer_token {
            url.query_pairs_mut().append_pair("token", token);
        }
        if let Some(params) = query_params {
            url.query_pairs_mut().extend_pairs(params);
        }
        // url to request, append header UA
        let mut request = IntoClientRequest::into_client_request(&url)?;
        request.headers_mut().insert(header::USER_AGENT, USER_AGENT.parse()?);
        debug!("create_stream, url: {}, headers: {:?}", url, request.headers());
        let (stream, _) = connect_async(request).await?;
        let stream = stream.filter_map(|msg| async {
            match msg {
                Ok(Message::Text(txt)) => match serde_json::from_str::<T>(&txt) {
                    Ok(v) => Some(Ok(v)),
                    Err(e) => Some(Err(anyhow!(e))),
                },
                _ => None,
            }
        });
        Ok(stream)
    }

    pub async fn get_logs(
        &self,
        level: Option<LogLevel>,
    ) -> Result<impl Stream<Item = Result<Log>>> {
        let params = level.map(|l| HashMap::from([("level".to_string(), l.to_string())]));
        self.create_stream::<Log>("/logs", params).await
    }

    pub async fn get_connections(&self) -> Result<impl Stream<Item = Result<ConnectionsWrapper>>> {
        self.create_stream::<ConnectionsWrapper>("/connections", None).await
    }

    pub async fn delete_connection(&self, id: &str) -> Result<()> {
        // NOTE `DELETE /connections/{id}` always returns empty body
        let _ = self
            .client
            .delete(self.api.join(&format!("/connections/{}", id))?)
            .send()
            .await
            .context("Fail to send `DELETE /connections/<id>` request")?
            .error_for_status()
            .context("Fail to request `DELETE /connections/<id>`")?
            .bytes()
            .await
            .context("Fail to read response of `DELETE /connections/<id>`");

        Ok(())
    }

    pub async fn get_memory(&self) -> Result<impl Stream<Item = Result<Memory>>> {
        self.create_stream::<Memory>("/memory", None).await
    }

    pub async fn get_traffic(&self) -> Result<impl Stream<Item = Result<Traffic>>> {
        self.create_stream::<Traffic>("/traffic", None).await
    }

    pub async fn get_proxies(&self) -> Result<IndexMap<String, Proxy>> {
        #[derive(Deserialize)]
        struct Wrapper {
            proxies: IndexMap<String, Proxy>,
        }

        let body = self
            .client
            .get(self.api.join("/proxies")?)
            .send()
            .await
            .context("Fail to send `GET /proxies`")?
            .error_for_status()
            .context("Fail to request `GET /proxies`")?
            .json::<Wrapper>()
            .await
            .context("Fail to parse response of `GET /proxies`")?;

        Ok(body.proxies)
    }

    pub async fn update_proxy(&self, selector_name: String, name: String) -> Result<()> {
        let body = serde_json::to_string(&json!({ "name": &name }))
            .with_context(|| format!("Fail to create body with name `{}`", name))?;
        let _ = self
            .client
            .put(self.api.join(&format!("/proxies/{}", selector_name))?)
            .body(body)
            .send()
            .await
            .context("Fail to send `PUT /proxies/<selector_name>` request")?
            .error_for_status()
            .context("Fail to request `PUT /connections/<selector_name>`")?
            .bytes()
            .await
            .context("Fail to read response of `PUT /connections/<selector_name>`");

        Ok(())
    }

    pub async fn test_proxy(&self, name: String, url: String, timeout: u64) -> Result<u16> {
        #[derive(Deserialize)]
        struct DelayResp {
            delay: u16,
        }

        let body = self
            .client
            .get(self.api.join(&format!("/proxies/{}/delay", name))?)
            .query(&[("url", url), ("timeout", timeout.to_string())])
            .send()
            .await
            .context("Fail to send `GET /proxies/<name>/delay`")?
            .error_for_status()
            .context("Fail to request `GET /proxies/<name>/delay`")?
            .json::<DelayResp>()
            .await
            .context("Fail to parse response of `GET /proxies/<name>/delay`")?;

        Ok(body.delay)
    }

    pub async fn test_proxy_group(
        &self,
        name: String,
        url: String,
        timeout: u64,
    ) -> Result<HashMap<String, u16>> {
        let body = self
            .client
            .get(self.api.join(&format!("/group/{}/delay", name))?)
            .query(&[("url", url), ("timeout", timeout.to_string())])
            .send()
            .await
            .context("Fail to send `GET /group/<name>/delay`")?
            .error_for_status()
            .context("Fail to request `GET /group/<name>/delay`")?
            .json()
            .await
            .context("Fail to parse response of `GET /group/<name>/delay`")?;

        Ok(body)
    }

    pub async fn get_providers(&self) -> Result<IndexMap<String, ProxyProvider>> {
        #[derive(Deserialize)]
        struct Wrapper {
            providers: IndexMap<String, ProxyProvider>,
        }

        let body = self
            .client
            .get(self.api.join("/providers/proxies")?)
            .send()
            .await
            .context("Fail to send `GET /providers/proxies`")?
            .error_for_status()
            .context("Fail to request `GET /providers/proxies`")?
            .json::<Wrapper>()
            .await
            .context("Fail to parse response of `GET /providers/proxies`")?;

        Ok(body.providers)
    }

    pub async fn health_check_provider<S: AsRef<str>>(&self, name: S) -> Result<()> {
        let _ = self
            .client
            .get(self.api.join(&format!("/providers/proxies/{}/healthcheck", name.as_ref()))?)
            .send()
            .await
            .context("Fail to send `GET /providers/proxies/<name>/healthcheck` request")?
            .error_for_status()
            .context("Fail to request `GET /providers/proxies/<name>/healthcheck`")?
            .bytes()
            .await
            .context("Fail to read response of `GET /providers/proxies/<name>/healthcheck`");

        Ok(())
    }

    pub async fn update_provider<S: AsRef<str>>(&self, name: S) -> Result<()> {
        let _ = self
            .client
            .put(self.api.join(&format!("/providers/proxies/{}", name.as_ref()))?)
            .send()
            .await
            .context("Fail to send `PUT /providers/proxies/<name>`")?
            .error_for_status()
            .context("Fail to request `PUT /providers/proxies/<name>`")?
            .bytes()
            .await
            .context("Fail to parse response of `PUT /providers/proxies/<name>`")?;

        Ok(())
    }

    pub async fn get_rules(&self) -> Result<Vec<Rule>> {
        #[derive(Deserialize)]
        struct Wrapper {
            rules: Vec<Rule>,
        }

        let body = self
            .client
            .get(self.api.join("/rules")?)
            .send()
            .await
            .context("Fail to send `GET /rules`")?
            .error_for_status()
            .context("Fail to request `GET /rules`")?
            .json::<Wrapper>()
            .await
            .context("Fail to parse response of `GET /rules`")?;

        Ok(body.rules)
    }

    pub async fn update_rules_disabled_state(&self, body: IndexMap<usize, bool>) -> Result<()> {
        let _ = self
            .client
            .patch(self.api.join("/rules/disable")?)
            .json(&body)
            .send()
            .await
            .context("Fail to send `PATCH /rules/disable` request")?
            .error_for_status()
            .context("Fail to request `PATCH /rules/disable`")?
            .bytes()
            .await
            .context("Fail to read response of `PATCH /rules/disable`");

        Ok(())
    }

    pub async fn get_rule_providers(&self) -> Result<IndexMap<String, RuleProvider>> {
        #[derive(Deserialize)]
        struct Wrapper {
            providers: IndexMap<String, RuleProvider>,
        }

        let body = self
            .client
            .get(self.api.join("/providers/rules")?)
            .send()
            .await
            .context("Fail to send `GET /providers/rules`")?
            .error_for_status()
            .context("Fail to request `GET /providers/rules`")?
            .json::<Wrapper>()
            .await
            .context("Fail to parse response of `GET /providers/rules`")?;

        Ok(body.providers)
    }

    pub async fn update_rule_provider<S: AsRef<str>>(&self, name: S) -> Result<()> {
        let _ = self
            .client
            .put(self.api.join(&format!("/providers/rules/{}", name.as_ref()))?)
            .send()
            .await
            .context("Fail to send `PUT /providers/rules/<name>` request")?
            .error_for_status()
            .context("Fail to request `PUT /providers/rules/<name>`")?
            .bytes()
            .await
            .context("Fail to read response of `PUT /providers/rules/<name>`");

        Ok(())
    }

    pub async fn get_core_config(&self) -> Result<CoreConfig> {
        let body = self
            .client
            .get(self.api.join("/configs")?)
            .send()
            .await
            .context("Fail to send `GET /configs`")?
            .error_for_status()
            .context("Fail to request `GET /configs`")?
            .json::<CoreConfig>()
            .await
            .context("Fail to parse response of `GET /configs`")?;

        Ok(body)
    }

    pub async fn update_core_config(&self, body: Vec<u8>) -> Result<()> {
        let _ = self
            .client
            .patch(self.api.join("/configs")?)
            .body(body)
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .send()
            .await
            .context("Fail to send `PATCH /configs` request")?
            .error_for_status()
            .context("Fail to request `PATCH /configs`")?
            .bytes()
            .await
            .context("Fail to read response of `PATCH /configs`");

        Ok(())
    }

    pub async fn reload_config(&self) -> Result<()> {
        let body = r#"{"path":"","payload":""}"#;
        let _ = self
            .client
            .put(self.api.join("/configs")?)
            .body(body)
            .query(&[("force", "true")])
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .send()
            .await
            .context("Fail to send `PUT /configs` request")?
            .error_for_status()
            .context("Fail to request `PUT /configs`")?
            .bytes()
            .await
            .context("Fail to read response of `PUT /configs`");

        Ok(())
    }

    pub async fn restart(&self) -> Result<()> {
        let _ = self
            .client
            .post(self.api.join("/restart")?)
            .send()
            .await
            .context("Fail to send `POST /restart` request")?
            .error_for_status()
            .context("Fail to request `POST /restart`")?
            .bytes()
            .await
            .context("Fail to read response of `POST /restart`");

        Ok(())
    }

    pub async fn flush_fake_ip_cache(&self) -> Result<()> {
        let _ = self
            .client
            .post(self.api.join("/cache/fakeip/flush")?)
            .send()
            .await
            .context("Fail to send `POST /cache/fakeip/flush` request")?
            .error_for_status()
            .context("Fail to request `POST /cache/fakeip/flush`")?
            .bytes()
            .await
            .context("Fail to read response of `POST /cache/fakeip/flush`");

        Ok(())
    }

    pub async fn flush_dns_cache(&self) -> Result<()> {
        let _ = self
            .client
            .post(self.api.join("/cache/dns/flush")?)
            .send()
            .await
            .context("Fail to send `POST /cache/dns/flush` request")?
            .error_for_status()
            .context("Fail to request `POST /cache/dns/flush`")?
            .bytes()
            .await
            .context("Fail to read response of `POST /cache/dns/flush`");

        Ok(())
    }

    pub async fn update_geo(&self) -> Result<()> {
        let _ = self
            .client
            .post(self.api.join("/configs/geo")?)
            .send()
            .await
            .context("Fail to send `POST /configs/geo` request")?
            .error_for_status()
            .context("Fail to request `POST /configs/geo`")?
            .bytes()
            .await
            .context("Fail to read response of `POST /configs/geo`");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Once};

    use futures_util::{StreamExt, future, pin_mut};
    use tokio::sync::mpsc;
    use tokio::time::sleep;
    use tokio_util::sync::CancellationToken;

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
    async fn test_update_core_config() {
        async fn get_tun_enable(api: &Api) -> Option<bool> {
            let config = api.get_core_config().await.unwrap();
            config.get("tun").and_then(|tun| tun.get("enable")).and_then(|v| v.as_bool())
        }
        init_logger();
        let api = init_api();
        let before = get_tun_enable(&api).await;
        debug!("core config .tun.enable before: {:?}", before);
        let body = format!(r#" {{ "tun": {{ "enable": {} }}}} "#, !before.unwrap_or_default())
            .into_bytes();
        println!("body: {}", String::from_utf8_lossy(&body));
        api.update_core_config(body).await.unwrap();
        sleep(std::time::Duration::from_secs(1)).await; // wait for config to apply
        let after = get_tun_enable(&api).await;
        debug!("core config .tun.enable after: {:?}", after);
        assert_ne!(before, after);
    }

    #[tokio::test]
    async fn test_get_core_config() {
        init_logger();
        let api = init_api();
        let config = api.get_core_config().await.unwrap();
        let tun = config.get("tun").unwrap();
        debug!("core config type: {}\n\t\t{:?}", std::any::type_name_of_val(&config), config);
        debug!("core config .tun type: {}\n\t\t{:?}", std::any::type_name_of_val(&tun), tun);
    }

    #[tokio::test]
    async fn test_update_rule_provider() {
        init_logger();
        let api = init_api();
        let providers = api.get_rule_providers().await.unwrap();
        if let Some(name) = providers.keys().next() {
            debug!("rule providers {name} updating...");
            api.update_rule_provider(name).await.unwrap();
            debug!("rule providers {name} updated");
        } else {
            debug!("no rule providers found");
        }
    }

    #[tokio::test]
    async fn test_get_rule_providers() {
        init_logger();
        let api = init_api();
        let providers = api.get_rule_providers().await.unwrap();
        debug!("rule providers: {providers:?}");
    }

    #[tokio::test]
    async fn test_get_rules() {
        init_logger();
        let api = init_api();
        let rules = api.get_rules().await.unwrap();
        debug!("rules: {rules:?}");
    }

    #[tokio::test]
    async fn test_test_proxy() {
        init_logger();
        let api = init_api();
        let delay = api
            .test_proxy("新加坡①一优化".into(), "https://www.gstatic.com/generate_204".into(), 5000)
            .await
            .unwrap();
        debug!("delay: {delay}");
    }

    #[tokio::test]
    async fn test_test_proxy_group() {
        init_logger();
        let api = init_api();
        let delay = api
            .test_proxy_group("新加坡".into(), "https://www.gstatic.com/generate_204".into(), 5000)
            .await
            .unwrap();
        debug!("delay: {delay:?}");
    }

    #[tokio::test]
    async fn test_get_proxies() {
        init_logger();
        let api = init_api();
        let proxies = api.get_proxies().await.unwrap();
        debug!("proxies: {proxies:?}");
    }

    #[tokio::test]
    async fn test_get_providers() {
        init_logger();
        let api = init_api();
        let providers = api.get_providers().await.unwrap();
        debug!("providers: {providers:?}");
    }

    #[tokio::test]
    async fn test_ws() {
        init_logger();
        let api = Arc::new(init_api());

        macro_rules! spawn_consumer {
            ($name:literal, $method:ident, $api:expr, $n:expr) => {{
                let api = Arc::clone(&$api);
                tokio::spawn(async move {
                    api.$method()
                        .await
                        .unwrap()
                        .take($n)
                        .for_each(|msg| {
                            debug!("[{:>12}]\tmsg: {:?}", $name, msg);
                            future::ready(())
                        })
                        .await
                })
            }};
        }

        let handles = vec![
            spawn_consumer!("memory", get_memory, api, 10),
            spawn_consumer!("traffic", get_traffic, api, 10),
        ];

        for h in handles {
            let _ = h.await;
        }
    }

    #[tokio::test]
    async fn test_get_connections() {
        init_logger();
        let api = init_api();

        let stream = api.get_connections().await.unwrap().take(10);
        pin_mut!(stream);
        while let Some(msg) = stream.next().await {
            let value = msg.unwrap().connections.unwrap()[0].metadata.clone();
            debug!("meta: {value:?}");
        }
    }

    #[tokio::test]
    async fn test_delete_connection() {
        init_logger();
        let api = init_api();
        let resp = api.delete_connection("756b7e9a-0c84-48b2-b135-e8dab13e3440").await;
        assert!(resp.is_ok());
    }

    #[tokio::test]
    async fn test_get_logs() {
        init_logger();
        let api = init_api();

        let token = CancellationToken::new();
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();

        let token_cloned = token.clone();
        tokio::task::Builder::new()
            .name("consumer")
            .spawn(async move {
                api.get_logs(Some(LogLevel::Debug))
                    .await
                    .unwrap()
                    .take_until(token_cloned.cancelled())
                    .for_each(|msg| {
                        msg_tx.send(msg).unwrap();
                        future::ready(())
                    })
                    .await
            })
            .unwrap();

        let mut cnt = 0;
        while let Some(msg) = msg_rx.recv().await {
            if cnt > 10 {
                token.cancel();
            }
            debug!("msg: {msg:?}");
            cnt += 1;
        }
    }

    #[tokio::test]
    async fn test_get_version() {
        let api = init_api();
        let version = api.get_version().await;
        assert!(version.is_ok());
    }

    fn init_api() -> Api {
        let config =
            Config::new(Some(PathBuf::from("/home/wsl/.config/mihomo-tui/config.yaml"))).unwrap();
        Api::new(&config).unwrap()
    }
}
