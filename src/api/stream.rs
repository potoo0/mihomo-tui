use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Result, anyhow};
use futures_util::{Stream, StreamExt, stream};
use reqwest::header;
use reqwest::header::HeaderValue;
use serde::de::DeserializeOwned;
use tokio::net::TcpStream;
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::client::Request;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use tracing::{debug, warn};

use super::{Api, USER_AGENT};
use crate::models::{ConnectionsWrapper, Log, LogLevel, Memory, Traffic};

const DEFAULT_WS_RETRY_INTERVAL: Duration = Duration::from_secs(3);

impl Api {
    fn build_ws_request(
        &self,
        path: &str,
        query_params: Option<HashMap<String, String>>,
    ) -> Result<Request> {
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
        request.headers_mut().insert(header::USER_AGENT, HeaderValue::from_static(USER_AGENT));
        debug!("create websocket stream, url: {}, headers: {:?}", url, request.headers());
        Ok(request)
    }

    pub fn create_stream<T>(
        &self,
        path: &str,
        query_params: Option<HashMap<String, String>>,
        retry_interval: Duration,
    ) -> Result<impl Stream<Item = Result<T>>>
    where
        T: DeserializeOwned,
    {
        struct ReconnectState {
            request: Request,
            retry_interval: Duration,
            ws: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
        }

        let request = self.build_ws_request(path, query_params)?;
        let state = ReconnectState { request, retry_interval, ws: None };

        Ok(stream::unfold(state, |mut state| async move {
            loop {
                if state.ws.is_none() {
                    match connect_async(state.request.clone()).await {
                        Ok((ws, _)) => {
                            state.ws = Some(ws);
                        }
                        Err(e) => {
                            warn!(
                                error = ?e,
                                retry_interval = ?state.retry_interval,
                                "Failed to connect websocket stream, retrying"
                            );
                            sleep(state.retry_interval).await;
                            continue;
                        }
                    }
                }

                let ws = state.ws.as_mut().unwrap();
                match ws.next().await {
                    Some(Ok(Message::Text(txt))) => {
                        let item = serde_json::from_str::<T>(&txt).map_err(anyhow::Error::from);
                        return Some((item, state));
                    }
                    Some(Ok(Message::Close(frame))) => {
                        warn!(
                            close_frame = ?frame,
                            retry_interval = ?state.retry_interval,
                            "Websocket stream closed by peer, retrying"
                        );
                        state.ws = None;
                        sleep(state.retry_interval).await;
                    }
                    Some(Ok(_)) => {
                        continue;
                    }
                    Some(Err(e)) => {
                        warn!(
                            error = ?e,
                            retry_interval = ?state.retry_interval,
                            "Websocket stream disconnected, retrying"
                        );
                        state.ws = None;
                        sleep(state.retry_interval).await;
                    }
                    None => {
                        warn!(
                            retry_interval = ?state.retry_interval,
                            "Websocket stream closed, retrying"
                        );
                        state.ws = None;
                        sleep(state.retry_interval).await;
                    }
                }
            }
        }))
    }

    pub async fn stream_logs(
        &self,
        level: Option<LogLevel>,
    ) -> Result<impl Stream<Item = Result<Log>>> {
        let params = level.map(|l| HashMap::from([("level".to_string(), l.to_string())]));
        self.create_stream::<Log>("/logs", params, DEFAULT_WS_RETRY_INTERVAL)
    }

    pub async fn stream_connections(
        &self,
    ) -> Result<impl Stream<Item = Result<ConnectionsWrapper>>> {
        self.create_stream::<ConnectionsWrapper>("/connections", None, DEFAULT_WS_RETRY_INTERVAL)
    }

    pub async fn stream_memory(&self) -> Result<impl Stream<Item = Result<Memory>>> {
        self.create_stream::<Memory>("/memory", None, DEFAULT_WS_RETRY_INTERVAL)
    }

    pub async fn stream_traffic(&self) -> Result<impl Stream<Item = Result<Traffic>>> {
        self.create_stream::<Traffic>("/traffic", None, DEFAULT_WS_RETRY_INTERVAL)
    }
}

#[cfg(test)]
mod reconnecting_stream_tests {
    use std::time::Duration;

    use futures_util::{SinkExt, StreamExt, pin_mut};
    use tokio::net::TcpListener;
    use tokio::time::timeout;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    use super::*;
    use crate::config::{BufferConfig, Config, ProxySetting, default_mihomo_repo};
    use crate::utils::test::init_logger;

    const TEST_CASES: [&str; 3] = ["t001", "t002", "t003"];
    const NEXT_TIMEOUT: Duration = Duration::from_secs(1);
    const RETRY_INTERVAL: Duration = Duration::from_millis(10);

    fn test_api(addr: std::net::SocketAddr) -> Api {
        let config = Config {
            mihomo_api: format!("http://{addr}").parse().unwrap(),
            mihomo_secret: None,
            mihomo_config_schema: None,
            mihomo_repo: default_mihomo_repo(),
            log_file: None,
            log_level: None,
            ui: None,
            proxy_setting: ProxySetting::default(),
            buffer: BufferConfig::default(),
        };
        Api::new(&config).unwrap()
    }

    fn log_message(payload: &str) -> Message {
        Message::Text(format!(r#"{{"type":"info","payload":"{payload}"}}"#).into())
    }

    async fn collect_payloads(api: Api, count: usize) -> Vec<String> {
        let stream = api.create_stream::<Log>("/logs", None, RETRY_INTERVAL).unwrap().take(count);
        pin_mut!(stream);

        let mut payloads = Vec::with_capacity(count);
        for _ in 0..count {
            let next = timeout(NEXT_TIMEOUT, stream.next()).await.unwrap();
            payloads.push(next.unwrap().unwrap().payload);
        }
        payloads
    }

    #[tokio::test]
    async fn create_stream_reconnects_after_close() {
        init_logger();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            for payload in TEST_CASES {
                let (tcp, _) = listener.accept().await.unwrap();
                let mut ws = accept_async(tcp).await.unwrap();
                ws.send(log_message(payload)).await.unwrap();
                ws.close(None).await.unwrap();
            }
        });

        let api = test_api(addr);
        let payloads = collect_payloads(api, TEST_CASES.len()).await;

        assert_eq!(payloads, TEST_CASES);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn create_stream_reconnects_after_abrupt_disconnect() {
        init_logger();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            for payload in TEST_CASES {
                let (tcp, _) = listener.accept().await.unwrap();
                let mut ws = accept_async(tcp).await.unwrap();
                ws.send(log_message(payload)).await.unwrap();
                drop(ws);
            }
        });

        let api = test_api(addr);
        let payloads = collect_payloads(api, TEST_CASES.len()).await;

        assert_eq!(payloads, TEST_CASES);
        server.await.unwrap();
    }
}
