use std::collections::HashMap;

use anyhow::{Context, Result};
use indexmap::IndexMap;
use reqwest::header::{CONTENT_TYPE, HeaderValue};
use serde::Deserialize;
use serde_json::json;

use super::Api;
use crate::models::dns::{DnsQueryRequest, DnsQueryResponse};
use crate::models::proxy::Proxy;
use crate::models::proxy_provider::ProxyProvider;
use crate::models::{ConnectionsWrapper, CoreConfig, Rule, RuleProvider, Version};

impl Api {
    pub async fn get_version(&self) -> Result<Version> {
        let resp = self
            .client
            .get(self.api.join("/version")?)
            .send()
            .await
            .context("Fail to send `GET /version`")?;

        let body = Self::check_status(resp)
            .await
            .context("Fail to request `GET /version`")?
            .json::<Version>()
            .await
            .context("Fail to parse response of `GET /version`")?;

        Ok(body)
    }

    pub async fn get_connections(&self) -> Result<ConnectionsWrapper> {
        let resp = self
            .client
            .get(self.api.join("/connections")?)
            .send()
            .await
            .context("Fail to send `GET /connections`")?;

        let body = Self::check_status(resp)
            .await
            .context("Fail to request `GET /connections`")?
            .json::<ConnectionsWrapper>()
            .await
            .context("Fail to parse response of `GET /connections`")?;

        Ok(body)
    }

    pub async fn delete_connection(&self, id: &str) -> Result<()> {
        // NOTE `DELETE /connections/{id}` always returns empty body
        let resp = self
            .client
            .delete(self.api.join(&format!("/connections/{}", id))?)
            .send()
            .await
            .context("Fail to send `DELETE /connections/<id>` request")?;

        let _ = Self::check_status(resp)
            .await
            .context("Fail to request `DELETE /connections/<id>`")?
            .bytes()
            .await
            .context("Fail to read response of `DELETE /connections/<id>`")?;

        Ok(())
    }

    pub async fn get_proxies(&self) -> Result<IndexMap<String, Proxy>> {
        #[derive(Deserialize)]
        struct Wrapper {
            proxies: IndexMap<String, Proxy>,
        }

        let resp = self
            .client
            .get(self.api.join("/proxies")?)
            .send()
            .await
            .context("Fail to send `GET /proxies`")?;

        let body = Self::check_status(resp)
            .await
            .context("Fail to request `GET /proxies`")?
            .json::<Wrapper>()
            .await
            .context("Fail to parse response of `GET /proxies`")?;

        Ok(body.proxies)
    }

    pub async fn update_proxy<S: AsRef<str>>(&self, selector_name: S, name: S) -> Result<()> {
        let body = serde_json::to_string(&json!({ "name": name.as_ref() }))
            .with_context(|| format!("Fail to create body with name `{}`", name.as_ref()))?;
        let resp = self
            .client
            .put(self.api.join(&format!("/proxies/{}", selector_name.as_ref()))?)
            .body(body)
            .send()
            .await
            .context("Fail to send `PUT /proxies/<selector_name>` request")?;

        let _ = Self::check_status(resp)
            .await
            .context("Fail to request `PUT /proxies/<selector_name>`")?
            .bytes()
            .await
            .context("Fail to read response of `PUT /proxies/<selector_name>`")?;

        Ok(())
    }

    pub async fn test_proxy<S: AsRef<str>>(&self, name: S, url: S, timeout: usize) -> Result<u16> {
        #[derive(Deserialize)]
        struct DelayResp {
            delay: u16,
        }

        let resp = self
            .client
            .get(self.api.join(&format!("/proxies/{}/delay", name.as_ref()))?)
            .query(&[("url", url.as_ref()), ("timeout", timeout.to_string().as_ref())])
            .send()
            .await
            .context("Fail to send `GET /proxies/<name>/delay`")?;

        let body = Self::check_status(resp)
            .await
            .context("Fail to request `GET /proxies/<name>/delay`")?
            .json::<DelayResp>()
            .await
            .context("Fail to parse response of `GET /proxies/<name>/delay`")?;

        Ok(body.delay)
    }

    pub async fn test_proxy_group<S: AsRef<str>>(
        &self,
        name: S,
        url: S,
        timeout: usize,
    ) -> Result<HashMap<String, u16>> {
        let resp = self
            .client
            .get(self.api.join(&format!("/group/{}/delay", name.as_ref()))?)
            .query(&[("url", url.as_ref()), ("timeout", timeout.to_string().as_ref())])
            .send()
            .await
            .context("Fail to send `GET /group/<name>/delay`")?;

        let body = Self::check_status(resp)
            .await
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

        let resp = self
            .client
            .get(self.api.join("/providers/proxies")?)
            .send()
            .await
            .context("Fail to send `GET /providers/proxies`")?;

        let body = Self::check_status(resp)
            .await
            .context("Fail to request `GET /providers/proxies`")?
            .json::<Wrapper>()
            .await
            .context("Fail to parse response of `GET /providers/proxies`")?;

        Ok(body.providers)
    }

    pub async fn health_check_provider<S: AsRef<str>>(&self, name: S) -> Result<()> {
        let resp = self
            .client
            .get(self.api.join(&format!("/providers/proxies/{}/healthcheck", name.as_ref()))?)
            .send()
            .await
            .context("Fail to send `GET /providers/proxies/<name>/healthcheck` request")?;

        let _ = Self::check_status(resp)
            .await
            .context("Fail to request `GET /providers/proxies/<name>/healthcheck`")?
            .bytes()
            .await
            .context("Fail to read response of `GET /providers/proxies/<name>/healthcheck`")?;

        Ok(())
    }

    pub async fn update_provider<S: AsRef<str>>(&self, name: S) -> Result<()> {
        let resp = self
            .client
            .put(self.api.join(&format!("/providers/proxies/{}", name.as_ref()))?)
            .send()
            .await
            .context("Fail to send `PUT /providers/proxies/<name>`")?;

        let _ = Self::check_status(resp)
            .await
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

        let resp = self
            .client
            .get(self.api.join("/rules")?)
            .send()
            .await
            .context("Fail to send `GET /rules`")?;

        let body = Self::check_status(resp)
            .await
            .context("Fail to request `GET /rules`")?
            .json::<Wrapper>()
            .await
            .context("Fail to parse response of `GET /rules`")?;

        Ok(body.rules)
    }

    pub async fn update_rules_disabled_state(&self, body: IndexMap<usize, bool>) -> Result<()> {
        let resp = self
            .client
            .patch(self.api.join("/rules/disable")?)
            .json(&body)
            .send()
            .await
            .context("Fail to send `PATCH /rules/disable` request")?;

        let _ = Self::check_status(resp)
            .await
            .context("Fail to request `PATCH /rules/disable`")?
            .bytes()
            .await
            .context("Fail to read response of `PATCH /rules/disable`")?;

        Ok(())
    }

    pub async fn get_rule_providers(&self) -> Result<IndexMap<String, RuleProvider>> {
        #[derive(Deserialize)]
        struct Wrapper {
            providers: IndexMap<String, RuleProvider>,
        }

        let resp = self
            .client
            .get(self.api.join("/providers/rules")?)
            .send()
            .await
            .context("Fail to send `GET /providers/rules`")?;

        let body = Self::check_status(resp)
            .await
            .context("Fail to request `GET /providers/rules`")?
            .json::<Wrapper>()
            .await
            .context("Fail to parse response of `GET /providers/rules`")?;

        Ok(body.providers)
    }

    pub async fn update_rule_provider<S: AsRef<str>>(&self, name: S) -> Result<()> {
        let resp = self
            .client
            .put(self.api.join(&format!("/providers/rules/{}", name.as_ref()))?)
            .send()
            .await
            .context("Fail to send `PUT /providers/rules/<name>` request")?;

        let _ = Self::check_status(resp)
            .await
            .context("Fail to request `PUT /providers/rules/<name>`")?
            .bytes()
            .await
            .context("Fail to read response of `PUT /providers/rules/<name>`")?;

        Ok(())
    }

    pub async fn get_core_config(&self) -> Result<CoreConfig> {
        let resp = self
            .client
            .get(self.api.join("/configs")?)
            .send()
            .await
            .context("Fail to send `GET /configs`")?;

        let body = Self::check_status(resp)
            .await
            .context("Fail to request `GET /configs`")?
            .json::<CoreConfig>()
            .await
            .context("Fail to parse response of `GET /configs`")?;

        Ok(body)
    }

    pub async fn update_core_config(&self, body: Vec<u8>) -> Result<()> {
        let resp = self
            .client
            .patch(self.api.join("/configs")?)
            .body(body)
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .send()
            .await
            .context("Fail to send `PATCH /configs` request")?;

        let _ = Self::check_status(resp)
            .await
            .context("Fail to request `PATCH /configs`")?
            .bytes()
            .await
            .context("Fail to read response of `PATCH /configs`")?;

        Ok(())
    }

    pub async fn reload_config(&self) -> Result<()> {
        let body = r#"{"path":"","payload":""}"#;
        let resp = self
            .client
            .put(self.api.join("/configs")?)
            .body(body)
            .query(&[("force", "true")])
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .send()
            .await
            .context("Fail to send `PUT /configs` request")?;

        let _ = Self::check_status(resp)
            .await
            .context("Fail to request `PUT /configs`")?
            .bytes()
            .await
            .context("Fail to read response of `PUT /configs`")?;

        Ok(())
    }

    pub async fn restart(&self) -> Result<()> {
        let resp = self
            .client
            .post(self.api.join("/restart")?)
            .send()
            .await
            .context("Fail to send `POST /restart` request")?;

        let _ = Self::check_status(resp)
            .await
            .context("Fail to request `POST /restart`")?
            .bytes()
            .await
            .context("Fail to read response of `POST /restart`")?;

        Ok(())
    }

    pub async fn upgrade_core(&self) -> Result<()> {
        let resp = self
            .client
            .post(self.api.join("/upgrade")?)
            .send()
            .await
            .context("Fail to send `POST /upgrade` request")?;

        let _ = Self::check_status(resp)
            .await
            .context("Fail to request `POST /upgrade`")?
            .bytes()
            .await
            .context("Fail to read response of `POST /upgrade`")?;

        Ok(())
    }

    pub async fn flush_fake_ip_cache(&self) -> Result<()> {
        let resp = self
            .client
            .post(self.api.join("/cache/fakeip/flush")?)
            .send()
            .await
            .context("Fail to send `POST /cache/fakeip/flush` request")?;

        let _ = Self::check_status(resp)
            .await
            .context("Fail to request `POST /cache/fakeip/flush`")?
            .bytes()
            .await
            .context("Fail to read response of `POST /cache/fakeip/flush`")?;

        Ok(())
    }

    pub async fn flush_dns_cache(&self) -> Result<()> {
        let resp = self
            .client
            .post(self.api.join("/cache/dns/flush")?)
            .send()
            .await
            .context("Fail to send `POST /cache/dns/flush` request")?;

        let _ = Self::check_status(resp)
            .await
            .context("Fail to request `POST /cache/dns/flush`")?
            .bytes()
            .await
            .context("Fail to read response of `POST /cache/dns/flush`")?;

        Ok(())
    }

    pub async fn update_geo(&self) -> Result<()> {
        let resp = self
            .client
            .post(self.api.join("/configs/geo")?)
            .send()
            .await
            .context("Fail to send `POST /configs/geo` request")?;

        let _ = Self::check_status(resp)
            .await
            .context("Fail to request `POST /configs/geo`")?
            .bytes()
            .await
            .context("Fail to read response of `POST /configs/geo`")?;

        Ok(())
    }

    pub async fn query_dns(&self, req: &DnsQueryRequest) -> Result<DnsQueryResponse> {
        let resp = self
            .client
            .get(self.api.join("/dns/query")?)
            .query(req)
            .send()
            .await
            .context("Fail to send `GET /dns/query`")?;

        let body = Self::check_status(resp)
            .await
            .context("Fail to request `GET /dns/query`")?
            .json::<DnsQueryResponse>()
            .await
            .context("Fail to parse response of `GET /dns/query`")?;

        Ok(body)
    }
}
