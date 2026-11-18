//! 用本机的小 HTTP 服务测抓取的边界：重定向到内网、重定向次数、正文上限、超时。
//! 本机地址默认是被拦的，这里用 `allow_cidrs` 只放行 127.0.0.1/32，其他内网地址照样拦。

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use unfurl::Error;
use unfurl::fetch::{Cidr, FetchConfig, Fetcher, RequestOpts};
use url::Url;

/// 每个连接都按 `respond(path)` 回一段原始 HTTP。
async fn serve(respond: fn(&str, u16) -> Vec<u8>) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = l.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                if path == "/slow" {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                let _ = s.write_all(&respond(&path, port)).await;
                let _ = s.shutdown().await;
            });
        }
    });
    port
}

fn redirect(loc: &str) -> Vec<u8> {
    format!("HTTP/1.1 302 Found\r\nLocation: {loc}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes()
}

fn ok(ct: &str, body: &[u8]) -> Vec<u8> {
    let mut v =
        format!("HTTP/1.1 200 OK\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
    v.extend_from_slice(body);
    v
}

fn routes(path: &str, port: u16) -> Vec<u8> {
    match path {
        "/to-metadata" => redirect("http://169.254.169.254/latest/meta-data/"),
        "/to-localhost" => redirect(&format!("http://localhost:{port}/page")),
        "/to-private" => redirect("http://10.0.0.1/"),
        "/to-file" => redirect("file:///etc/passwd"),
        "/loop" => redirect("/loop"),
        "/chain3" => redirect("/chain2"),
        "/chain2" => redirect("/chain1"),
        "/chain1" => redirect("/page"),
        "/page" | "/slow" => ok("text/html", b"<title>ok</title>"),
        "/big.json" => ok("application/json", &vec![b' '; 3 * 1024 * 1024]),
        "/big.html" => {
            let mut b = b"<html><head><title>big</title></head><body>".to_vec();
            b.extend(std::iter::repeat_n(b'x', 3 * 1024 * 1024));
            ok("text/html", &b)
        }
        _ => ok("text/plain", b"?"),
    }
}

fn fetcher(timeout: Duration) -> Fetcher {
    Fetcher::new(FetchConfig { allow_cidrs: vec![Cidr::parse("127.0.0.1/32").unwrap()], timeout, ..FetchConfig::default() }).unwrap()
}

fn u(port: u16, path: &str) -> Url {
    Url::parse(&format!("http://127.0.0.1:{port}{path}")).unwrap()
}

#[tokio::test]
async fn redirects_are_rechecked() {
    let port = serve(routes).await;
    let f = fetcher(Duration::from_secs(8));
    for p in ["/to-metadata", "/to-localhost", "/to-private", "/to-file"] {
        let r = f.get(&u(port, p)).await;
        assert!(matches!(r, Err(Error::Blocked { .. })), "{p}: {r:?}");
    }
    let r = f.get(&u(port, "/chain3")).await.unwrap();
    assert_eq!(r.redirects, 3);
    assert_eq!(r.url.path(), "/page");
    assert!(matches!(f.get(&u(port, "/loop")).await, Err(Error::TooManyRedirects(5))));
}

#[tokio::test]
async fn body_cap() {
    let port = serve(routes).await;
    let f = fetcher(Duration::from_secs(8));
    let r = f.request(&u(port, "/big.json"), RequestOpts::json()).await;
    assert!(matches!(r, Err(Error::BodyTooLarge(_))), "{r:?}");
    let r = f.get(&u(port, "/big.html")).await.unwrap();
    assert!(r.truncated);
    assert_eq!(r.body.len(), 2 * 1024 * 1024);
}

#[tokio::test]
async fn total_timeout() {
    let port = serve(routes).await;
    let f = fetcher(Duration::from_millis(500));
    let t = std::time::Instant::now();
    let r = f.get(&u(port, "/slow")).await;
    assert!(matches!(r, Err(Error::Timeout)), "{r:?}");
    assert!(t.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn loopback_blocked_without_exception() {
    let port = serve(routes).await;
    let f = Fetcher::new(FetchConfig::default()).unwrap();
    assert!(matches!(f.get(&u(port, "/page")).await, Err(Error::Blocked { .. })));
}
