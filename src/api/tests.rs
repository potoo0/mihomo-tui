use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::*;
use crate::api::test_support::test_api;

async fn serve_version_request<S>(mut stream: S)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut chunk).await.unwrap();
        assert_ne!(read, 0);
        request.extend_from_slice(&chunk[..read]);
    }

    let request = String::from_utf8(request).unwrap();
    assert!(request.starts_with("GET /version HTTP/1.1\r\n"), "{request}");
    assert!(!request.to_ascii_lowercase().contains("authorization:"), "{request}");

    let body = r#"{"meta":true,"version":"test"}"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await.unwrap();
}

#[cfg(unix)]
mod unix_socket {
    use tokio::net::UnixListener;

    use super::*;

    #[tokio::test]
    async fn rest_request_uses_unix_socket_without_secret() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mihomo.sock");
        let listener = UnixListener::bind(&path).unwrap();

        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            serve_version_request(socket).await;
        });

        let version = test_api(MihomoApiEndpoint::UnixSocket(path), Some("must-not-be-sent"))
            .get_version()
            .await
            .unwrap();
        assert!(version.meta);
        assert_eq!(version.version, "test");
        server.await.unwrap();
    }
}

#[cfg(windows)]
mod windows_named_pipe {
    use std::time::{SystemTime, UNIX_EPOCH};

    use tokio::net::windows::named_pipe::ServerOptions;

    use super::*;

    fn unique_pipe_name() -> String {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        format!(r"\\.\pipe\mihomo-tui-{}-{nanos}", std::process::id())
    }

    #[tokio::test]
    async fn rest_request_uses_named_pipe_without_secret() {
        let pipe = unique_pipe_name();
        let mut server = ServerOptions::new().create(&pipe).unwrap();
        let api =
            test_api(MihomoApiEndpoint::WindowsNamedPipe(pipe.clone()), Some("must-not-be-sent"));

        let server_task = tokio::spawn(async move {
            server.connect().await.unwrap();
            serve_version_request(server).await;
        });

        let version = api.get_version().await.unwrap();
        assert!(version.meta);
        assert_eq!(version.version, "test");
        server_task.await.unwrap();
    }
}
