# rust-mount：把 Open Agent Docs 挂进自己的 axum 服务

宿主是一个 Rust（axum）服务，已经有自己的登录。做法：

1. 依赖 `oad-core`、`oad-server`；
2. 实现自己的 `AuthProvider`（`src/lib.rs` 的 `DemoAuth`：从宿主的会话认出是谁，按外部 id 在 Open Agent Docs 里找到 / 建用户，加进默认工作区），
   用 `AuthConfig::Custom(...)` 交给 `Core::open_full`；
3. `oad_server::router(AppState::new(core, ServerConfig::default()))` 和自己的路由 `merge` 在一起（要只出接口、不出网页用 `oad_server::api_router`）。

```sh
cargo run -p oad-example-rust-mount        # 打开 http://127.0.0.1:3000/host
cargo test -p oad-example-rust-mount       # 冒烟测试
```

这里的 cookie 登录只是演示，真实服务在 `authenticate` 里验自己的会话。
