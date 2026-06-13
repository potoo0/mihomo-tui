use std::path::PathBuf;
use std::sync::Arc;

use futures_util::{StreamExt, future, pin_mut};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use super::*;
use crate::config::load as load_config;
use crate::models::LogLevel;
use crate::models::dns::{DnsQueryRequest, DnsRecordType};
use crate::utils::test::init_logger;

#[tokio::test]
async fn test_query_dns() {
    init_logger();
    let api = init_api();
    let req = DnsQueryRequest { name: "google.com".to_string(), r#type: DnsRecordType::Aaaa };
    let response = api.query_dns(&req).await.unwrap();
    info!("{:#?}", response);
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
    let body =
        format!(r#" {{ "tun": {{ "enable": {} }}}} "#, !before.unwrap_or_default()).into_bytes();
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
        .test_proxy("美国-洛杉矶-自建", "https://www.gstatic.com/generate_204", 5000)
        .await
        .unwrap();
    debug!("delay: {delay}");
}

#[tokio::test]
async fn test_test_proxy_group() {
    init_logger();
    let api = init_api();
    let delay =
        api.test_proxy_group("新加坡", "https://www.gstatic.com/generate_204", 5000).await.unwrap();
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
        spawn_consumer!("memory", stream_memory, api, 2),
        spawn_consumer!("traffic", stream_traffic, api, 2),
    ];

    for h in handles {
        let _ = h.await;
    }
}

#[tokio::test]
async fn test_get_connections() {
    init_logger();
    let api = init_api();
    let conns = api.get_connections().await.unwrap();
    debug!("connections: {:?}", conns.connections);
}

#[tokio::test]
async fn test_stream_connections() {
    init_logger();
    let api = init_api();

    let stream = api.stream_connections().await.unwrap().take(2);
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
async fn test_stream_logs() {
    init_logger();
    let api = init_api();

    let token = CancellationToken::new();
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();

    let token_cloned = token.clone();
    tokio::task::Builder::new()
        .name("consumer")
        .spawn(async move {
            api.stream_logs(Some(LogLevel::Debug))
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
        if cnt > 2 {
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
        load_config(Some(PathBuf::from("/home/wsl/.config/mihomo-tui/config.yaml"))).unwrap();
    Api::new(&config).unwrap()
}
