//! `cargo run -p oad-example-rust-mount`，然后打开 http://127.0.0.1:3000/host
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), String> {
    let dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data-rust-mount".into());
    let (app, core) = oad_example_rust_mount::app(std::path::Path::new(&dir)).await?;
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let l = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| e.to_string())?;
    println!("http://127.0.0.1:{port}/host");
    axum::serve(l, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|e| e.to_string())?;
    core.shutdown().await;
    Ok(())
}
