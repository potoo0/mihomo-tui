use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use futures_util::{Stream, StreamExt, stream};
use reqwest::header;
use reqwest::header::HeaderValue;
use serde::de::DeserializeOwned;
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::error::Error as WebSocketError;
use tokio_tungstenite::tungstenite::handshake::client::Request;
use tokio_tungstenite::{client_async, connect_async};
use tracing::{debug, warn};

use super::{Api, USER_AGENT};
use crate::config::MihomoApiEndpoint;
use crate::models::{ConnectionsWrapper, Log, LogLevel, Memory, Traffic};

const DEFAULT_WS_RETRY_INTERVAL: Duration = Duration::from_secs(3);

type WebSocketMessageStream =
    Pin<Box<dyn Stream<Item = std::result::Result<Message, WebSocketError>> + Send>>;

async fn connect_websocket(
    endpoint: &MihomoApiEndpoint,
    request: Request,
) -> Result<WebSocketMessageStream> {
    match endpoint {
        MihomoApiEndpoint::Http(_) => {
            let (ws, _) = connect_async(request).await.context("Fail to connect websocket")?;
            Ok(Box::pin(ws))
        }
        MihomoApiEndpoint::UnixSocket(path) => {
            #[cfg(unix)]
            {
                let socket = tokio::net::UnixStream::connect(path)
                    .await
                    .with_context(|| format!("Fail to connect Unix socket `{}`", path.display()))?;
                let (ws, _) = client_async(request, socket)
                    .await
                    .context("Fail to complete websocket handshake over Unix socket")?;
                Ok(Box::pin(ws))
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
                let socket = tokio::net::windows::named_pipe::ClientOptions::new()
                    .open(pipe)
                    .with_context(|| format!("Fail to connect Windows named pipe `{pipe}`"))?;
                let (ws, _) = client_async(request, socket)
                    .await
                    .context("Fail to complete websocket handshake over Windows named pipe")?;
                Ok(Box::pin(ws))
            }
            #[cfg(not(windows))]
            anyhow::bail!(
                "Windows named pipe mihomo API `{pipe}` is not supported on this platform"
            )
        }
    }
}

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
            endpoint: MihomoApiEndpoint,
            request: Request,
            retry_interval: Duration,
            ws: Option<WebSocketMessageStream>,
        }

        let request = self.build_ws_request(path, query_params)?;
        let state =
            ReconnectState { endpoint: self.endpoint.clone(), request, retry_interval, ws: None };

        Ok(stream::unfold(state, |mut state| async move {
            loop {
                if state.ws.is_none() {
                    match connect_websocket(&state.endpoint, state.request.clone()).await {
                        Ok(ws) => {
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
    use crate::api::test_support::test_api;
    use crate::utils::test::init_logger;

    const TEST_CASES: [&str; 3] = ["t001", "t002", "t003"];
    const NEXT_TIMEOUT: Duration = Duration::from_secs(1);
    const RETRY_INTERVAL: Duration = Duration::from_millis(10);

    #[cfg(windows)]
    fn unique_pipe_name() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};

        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        format!(r"\\.\pipe\mihomo-tui-ws-{}-{nanos}", std::process::id())
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

        let api =
            test_api(MihomoApiEndpoint::Http(format!("http://{addr}").parse().unwrap()), None);
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

        let api =
            test_api(MihomoApiEndpoint::Http(format!("http://{addr}").parse().unwrap()), None);
        let payloads = collect_payloads(api, TEST_CASES.len()).await;

        assert_eq!(payloads, TEST_CASES);
        server.await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn create_stream_uses_unix_socket() {
        use tokio::net::UnixListener;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mihomo.sock");
        let listener = UnixListener::bind(&path).unwrap();

        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(socket).await.unwrap();
            ws.send(log_message(TEST_CASES[0])).await.unwrap();
            ws.close(None).await.unwrap();
        });

        let payloads = collect_payloads(
            test_api(MihomoApiEndpoint::UnixSocket(path), Some("must-not-be-sent")),
            1,
        )
        .await;
        assert_eq!(payloads, &TEST_CASES[..1]);
        server.await.unwrap();
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn create_stream_uses_windows_named_pipe() {
        use tokio::net::windows::named_pipe::ServerOptions;

        let pipe = unique_pipe_name();
        let mut server = ServerOptions::new().create(&pipe).unwrap();
        let api =
            test_api(MihomoApiEndpoint::WindowsNamedPipe(pipe.clone()), Some("must-not-be-sent"));

        let server_task = tokio::spawn(async move {
            server.connect().await.unwrap();
            let mut ws = accept_async(server).await.unwrap();
            ws.send(log_message(TEST_CASES[0])).await.unwrap();
            ws.close(None).await.unwrap();
        });

        let payloads = collect_payloads(api, 1).await;
        assert_eq!(payloads, &TEST_CASES[..1]);
        server_task.await.unwrap();
    }
}
