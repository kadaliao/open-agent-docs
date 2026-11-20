//! 启动 Open Agent Docs 服务器：命令行程序、Node 绑定（oad-node）、Python 绑定（oad-py）都用这里，配置项一样
//! （`OAD_*`，见 docs/zh/configuration.md）：先看调用方给的，再看进程的环境变量。
pub mod channel;

use oad_core::s3::{S3Blobs, S3Config};
use oad_core::storage::pg::Postgres;
use oad_core::storage::sqlite::Sqlite;
use oad_core::{
    AuthConfig, BlobStore, Core, FsBlobs, HubConfig, MediaConfig, NotifyConfig, Storage,
};
use oad_server::tenant::{ByHeader, ByHost, TenantResolver};
use oad_server::{AppState, ServerConfig};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

/// 配置：`OAD_*` 名字 → 值。调用方给的优先，没有再看进程的环境变量（`process_env` 为 false 时不看）。
#[derive(Clone, Debug, Default)]
pub struct Env {
    pub vars: HashMap<String, String>,
    pub process_env: bool,
}

impl Env {
    /// 只看进程的环境变量。
    pub fn from_process() -> Env {
        Env {
            vars: HashMap::new(),
            process_env: true,
        }
    }

    /// 给的这些，再加进程的环境变量。名字可以写 `OAD_DATA_DIR`、`data_dir` 或 `dataDir`。
    pub fn with(vars: impl IntoIterator<Item = (String, String)>) -> Env {
        Env {
            vars: vars.into_iter().map(|(k, v)| (key_name(&k), v)).collect(),
            process_env: true,
        }
    }

    pub fn get(&self, k: &str) -> Option<String> {
        self.vars
            .get(k)
            .cloned()
            .or_else(|| self.process_env.then(|| std::env::var(k).ok()).flatten())
            .filter(|v| !v.is_empty())
    }

    pub fn set(&mut self, k: &str, v: impl Into<String>) {
        self.vars.insert(k.to_string(), v.into());
    }

    fn flag(&self, k: &str) -> bool {
        self.get(k)
            .is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
    }

    fn list(&self, k: &str) -> Vec<String> {
        self.get(k)
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn num<T: std::str::FromStr>(&self, k: &str, default: T, what: &str) -> Result<T, String> {
        match self.get(k) {
            Some(v) => v.trim().parse().map_err(|_| format!("{k} 要是{what}")),
            None => Ok(default),
        }
    }
}

/// `dataDir` / `data_dir` / `OAD_DATA_DIR` → `OAD_DATA_DIR`。
pub fn key_name(k: &str) -> String {
    if k.starts_with("OAD_") {
        return k.to_string();
    }
    let mut out = String::from("OAD_");
    for (i, c) in k.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 && !out.ends_with('_') {
            out.push('_');
        }
        if c == '-' {
            out.push('_');
        } else {
            out.push(c.to_ascii_uppercase());
        }
    }
    out
}

/// 存储：`OAD_DATABASE_URL` 是 postgres:// 就用 Postgres，否则用数据目录里的 oad.db（SQLite）。
/// 数据目录照样放上传的文件和搜索索引。
/// 多租户时每个租户一个 schema（`t_<租户>`）。
pub async fn open_storage(
    env: &Env,
    data_dir: &Path,
    tenant: Option<&str>,
) -> Result<Arc<dyn Storage>, String> {
    std::fs::create_dir_all(data_dir).map_err(|e| format!("建数据目录失败：{e}"))?;
    if let Some(url) = env.get("OAD_DATABASE_URL")
        && (url.starts_with("postgres://") || url.starts_with("postgresql://"))
    {
        let schema = tenant.map(|t| format!("t_{}", t.replace('-', "_")));
        let s = Postgres::connect_with(&url, schema.as_deref(), 16)
            .await
            .map_err(|e| format!("连 Postgres 失败：{e}"))?;
        info!(tenant = tenant.unwrap_or("-"), "存储：Postgres");
        return Ok(Arc::new(s));
    }
    Sqlite::open(data_dir.join("oad.db"))
        .map(|s| Arc::new(s) as Arc<dyn Storage>)
        .map_err(|e| e.to_string())
}

/// 抓外部网页的配置：`OAD_FETCH_PROXY`、`OAD_FETCH_ALLOW_CIDRS`。
fn oad_unfurl_config() -> oad_unfurl::FetchConfig {
    oad_unfurl::FetchConfig::from_env()
}

/// 文件存储：`OAD_BLOB_URL=s3://…` 用 S3（多租户时前缀后面加租户名），否则数据目录里的 blobs/。
fn open_blobs(
    env: &Env,
    data_dir: &Path,
    tenant: Option<&str>,
) -> Result<Arc<dyn BlobStore>, String> {
    if let Some(u) = env.get("OAD_BLOB_URL") {
        let mut cfg = S3Config::from_url(&u).map_err(|e| e.to_string())?;
        if let Some(t) = tenant {
            cfg.prefix = if cfg.prefix.is_empty() {
                t.to_string()
            } else {
                format!("{}/{t}", cfg.prefix)
            };
        }
        info!(bucket = %cfg.bucket, prefix = %cfg.prefix, "文件存储：S3");
        return Ok(Arc::new(S3Blobs::new(cfg).map_err(|e| e.to_string())?));
    }
    Ok(Arc::new(
        FsBlobs::new(data_dir.join("blobs")).map_err(|e| e.to_string())?,
    ))
}

/// 一个租户（单租户时就是唯一的那个）用到的配置。
#[derive(Clone)]
struct Setup {
    env: Env,
    auth: AuthConfig,
    notify: NotifyConfig,
    max_upload: usize,
    server: ServerConfig,
}

/// 建一套服务：存储、文件、Core；本地登录时按 OAD_ADMIN_* 建管理员。
async fn build_core(
    setup: &Setup,
    data_dir: &Path,
    tenant: Option<&str>,
) -> Result<Arc<Core>, String> {
    let env = &setup.env;
    let storage = open_storage(env, data_dir, tenant).await?;
    let media = MediaConfig {
        blobs: open_blobs(env, data_dir, tenant)?,
        fetch: oad_unfurl_config(),
        max_upload: setup.max_upload,
        unfurl: true,
        search_dir: Some(data_dir.join("search")),
    };
    let core = Core::open_full(
        storage,
        setup.auth.clone(),
        HubConfig::default(),
        setup.notify.clone(),
        media,
    )
    .await
    .map_err(|e| e.to_string())?;
    if matches!(setup.auth, AuthConfig::Local) {
        match (env.get("OAD_ADMIN_EMAIL"), env.get("OAD_ADMIN_PASSWORD")) {
            (Some(email), Some(pw)) => {
                if core
                    .ensure_admin(&email, &pw)
                    .await
                    .map_err(|e| e.to_string())?
                {
                    info!(%email, tenant = tenant.unwrap_or("-"), "已建管理员");
                }
            }
            _ => {
                if core
                    .storage
                    .count_users()
                    .await
                    .map_err(|e| e.to_string())?
                    == 0
                {
                    warn!(
                        "还没有用户：设 OAD_ADMIN_EMAIL / OAD_ADMIN_PASSWORD 后重启，或用 `open-agent-docs user add`"
                    );
                }
            }
        }
    }
    Ok(core)
}

/// `OAD_TENANTS`：`host:<base 域名>`（`<租户>.<base>`，base 本身是 `main` 租户）或 `header:<请求头名>`
/// （配 `OAD_TENANT_SECRET` 时还要 `X-OAD-Tenant-Secret`）。不设就是单租户。
fn tenant_resolver(env: &Env) -> Result<Option<Box<dyn TenantResolver>>, String> {
    let Some(v) = env.get("OAD_TENANTS") else {
        return Ok(None);
    };
    match v.split_once(':') {
        Some(("host", base)) if !base.is_empty() => Ok(Some(Box::new(ByHost {
            base: base.to_string(),
            default: Some("main".into()),
        }))),
        Some(("header", h)) if !h.is_empty() => Ok(Some(Box::new(ByHeader {
            header: h.to_ascii_lowercase(),
            secret: env.get("OAD_TENANT_SECRET"),
            default: None,
        }))),
        _ => Err("OAD_TENANTS 要是 host:<域名> 或 header:<请求头名>".into()),
    }
}

/// 跑起来的服务器。
pub struct Running {
    pub addr: SocketAddr,
    /// `http://127.0.0.1:<端口>`（监听 0.0.0.0 时也写成 127.0.0.1）
    pub url: String,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
    cores: Arc<std::sync::Mutex<Vec<Arc<Core>>>>,
}

impl Running {
    /// 服务器自己停了（出错）时返回。
    pub async fn stopped(&mut self) -> Result<(), String> {
        match (&mut self.server).await {
            Ok(r) => r.map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// 关：先让 SSE / WebSocket 结束、页面存好，再停 HTTP；都有时限。
    pub async fn shutdown(mut self) -> Result<(), String> {
        let all: Vec<Arc<Core>> = self.cores.lock().unwrap().clone();
        let down = async move {
            for c in all {
                c.shutdown().await;
            }
        };
        if tokio::time::timeout(Duration::from_secs(30), down)
            .await
            .is_err()
        {
            error!("30 秒内没存完打开的页面（存储一直出错？），强制退出");
        }
        if let Some(s) = self.stop.take() {
            let _ = s.send(());
        }
        match tokio::time::timeout(Duration::from_secs(10), &mut self.server).await {
            Ok(Ok(r)) => r.map_err(|e| e.to_string()),
            Ok(Err(e)) => Err(e.to_string()),
            Err(_) => {
                warn!("还有连接 10 秒内没结束，强制退出");
                Ok(())
            }
        }
    }
}

/// 按配置建好、监听（`OAD_PORT=0` 时随便挑一个空端口），返回跑起来的服务器。
pub async fn start(env: &Env) -> Result<Running, String> {
    let auth = match env.get("OAD_AUTH").as_deref().unwrap_or("local") {
        "local" => AuthConfig::Local,
        "header" => AuthConfig::Header {
            secret: env
                .get("OAD_HEADER_SECRET")
                .ok_or("header 模式要设 OAD_HEADER_SECRET")?,
        },
        "none" => AuthConfig::None,
        "jwt" => AuthConfig::Jwt(oad_core::jwt::JwtConfig {
            secret: env.get("OAD_JWT_SECRET"),
            jwks_url: env.get("OAD_JWT_JWKS_URL"),
            issuer: env.get("OAD_JWT_ISSUER"),
            audience: env.get("OAD_JWT_AUDIENCE"),
            leeway: env.num("OAD_JWT_LEEWAY", 60, "秒数")?,
        }),
        other => {
            return Err(format!(
                "OAD_AUTH 只能是 local、header、jwt、none，不是 {other}"
            ));
        }
    };
    let host = env.get("OAD_HOST").unwrap_or_else(|| "127.0.0.1".into());
    let port: u16 = env.num("OAD_PORT", 8848, "端口号")?;
    let data_dir = PathBuf::from(env.get("OAD_DATA_DIR").unwrap_or_else(|| "./data".into()));
    let public_url = env.get("OAD_PUBLIC_URL");
    let notify = NotifyConfig {
        public_url: public_url.clone(),
        allow_private: env.flag("OAD_WEBHOOK_ALLOW_PRIVATE"),
        ..NotifyConfig::default()
    };
    if notify.allow_private {
        warn!("OAD_WEBHOOK_ALLOW_PRIVATE：webhook 可以投到内网地址（只在本机开发时用）");
    }
    let max_upload_mb: usize = env.num("OAD_MAX_UPLOAD_MB", 100, "整数")?;
    if matches!(auth, AuthConfig::None) && host != "127.0.0.1" && host != "localhost" {
        warn!(host = %host, "不登录模式监听在非本机地址上，任何能连上的人都能读写");
    }
    let mode = env.get("OAD_AUTH").unwrap_or_else(|| "local".into());
    let mut setup = Setup {
        env: env.clone(),
        auth: auth.clone(),
        notify,
        max_upload: max_upload_mb << 20,
        server: ServerConfig {
            web_dir: env.get("OAD_WEB_DIR").map(PathBuf::from),
            cookie_secure: env.flag("OAD_COOKIE_SECURE"),
            public_url,
            allowed_hosts: env.list("OAD_ALLOWED_HOSTS"),
            allowed_origins: env.list("OAD_ALLOWED_ORIGINS"),
            rate_limit: env.num("OAD_RATE_LIMIT", 1200, "整数（每分钟多少次，0 不限）")?,
            signed_url_ttl: env
                .num::<u64>("OAD_SIGNED_URL_TTL", 86400, "正整数（秒）")?
                .max(1),
            ..ServerConfig::default()
        },
    };
    // 单租户：一套；多租户：用到哪个租户建哪个，数据在 <数据目录>/tenants/<租户>/
    let cores: Arc<std::sync::Mutex<Vec<Arc<Core>>>> = Arc::default();
    let signup = env.get("OAD_SIGNUP");
    let app = match tenant_resolver(env)? {
        None => {
            let core = build_core(&setup, &data_dir, None).await?;
            apply_signup(&core, signup.as_deref());
            cores.lock().unwrap().push(core.clone());
            oad_server::router(AppState::new(core, setup.server.clone()))
        }
        Some(resolver) => {
            // 租户的 Host 各不一样：Host 检查交给租户解析（认不出的 404）
            setup.server.allowed_hosts = vec!["*".into()];
            let (root, cores2) = (data_dir.join("tenants"), cores.clone());
            let factory: oad_server::tenant::TenantFactory = Arc::new(move |t: String| {
                let (setup, root, cores, signup) =
                    (setup.clone(), root.clone(), cores2.clone(), signup.clone());
                Box::pin(async move {
                    let core = build_core(&setup, &root.join(&t), Some(&t)).await?;
                    apply_signup(&core, signup.as_deref());
                    cores.lock().unwrap().push(core.clone());
                    info!(tenant = %t, "租户已启动");
                    Ok(oad_server::router(AppState::new(
                        core,
                        setup.server.clone(),
                    )))
                })
            });
            info!("多租户模式");
            oad_server::tenant::multi_tenant_boxed(resolver, factory)
        }
    };
    let listener = tokio::net::TcpListener::bind((host.as_str(), port))
        .await
        .map_err(|e| format!("监听 {host}:{port} 失败：{e}"))?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    let shown = if host == "0.0.0.0" || host == "::" {
        "127.0.0.1".to_string()
    } else {
        host.clone()
    };
    let url = format!("http://{shown}:{}", addr.port());
    info!("在 {url} 上服务（{mode} 模式）");
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            let _ = stopped.await;
        })
        .into_future(),
    );
    Ok(Running {
        addr,
        url,
        stop: Some(stop),
        server,
        cores,
    })
}

/// `OAD_SIGNUP` 也可以是调用方给的（core 自己只看进程的环境变量）。
fn apply_signup(core: &Core, v: Option<&str>) {
    use oad_core::members::SignupMode;
    match v {
        Some("open") => core.set_signup_mode(SignupMode::Open),
        Some("off") => core.set_signup_mode(SignupMode::Off),
        Some(_) => core.set_signup_mode(SignupMode::Invite),
        None => {}
    }
}

/// 在自己的线程、自己的 tokio 运行时上跑的服务器（给 Node / Python 绑定用：它们的线程不归我们管）。
pub struct Background {
    pub url: String,
    pub port: u16,
    stop: Option<std::sync::mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<Result<(), String>>>,
}

impl Background {
    /// 起一个线程，在上面建运行时、启动服务器；监听好了才返回。
    pub fn start(env: Env) -> Result<Background, String> {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(String, u16), String>>();
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("open-agent-docs".into())
            .spawn(move || -> Result<(), String> {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .thread_name("open-agent-docs-worker")
                    .build()
                    .map_err(|e| e.to_string())?;
                rt.block_on(async move {
                    let running = match start(&env).await {
                        Ok(r) => r,
                        Err(e) => {
                            let _ = ready_tx.send(Err(e.clone()));
                            return Err(e);
                        }
                    };
                    let _ = ready_tx.send(Ok((running.url.clone(), running.addr.port())));
                    // 等关的信号（在阻塞线程上等，别占着运行时）
                    let _ = tokio::task::spawn_blocking(move || stop_rx.recv()).await;
                    running.shutdown().await
                })
            })
            .map_err(|e| e.to_string())?;
        match ready_rx.recv() {
            Ok(Ok((url, port))) => Ok(Background {
                url,
                port,
                stop: Some(stop_tx),
                thread: Some(thread),
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err(thread
                .join()
                .map_err(|_| "服务器线程崩了".to_string())?
                .err()
                .unwrap_or_default()),
        }
    }

    /// 关掉、等线程结束（页面都存好）。关过了再调什么也不做。
    pub fn close(&mut self) -> Result<(), String> {
        if let Some(s) = self.stop.take() {
            let _ = s.send(());
        }
        match self.thread.take() {
            Some(t) => t.join().map_err(|_| "服务器线程崩了".to_string())?,
            None => Ok(()),
        }
    }
}

impl Drop for Background {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

/// 打日志（只装一次；宿主自己装过就算了）。`RUST_LOG` 控制级别，默认 info。
pub fn init_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .try_init();
}

/// 日志写 stderr（stdout 留给 stdio 协议）。
pub fn init_logging_stderr() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(key_name("dataDir"), "OAD_DATA_DIR");
        assert_eq!(key_name("data_dir"), "OAD_DATA_DIR");
        assert_eq!(key_name("OAD_PORT"), "OAD_PORT");
        assert_eq!(key_name("jwtJwksUrl"), "OAD_JWT_JWKS_URL");
        assert_eq!(key_name("auth"), "OAD_AUTH");
    }

    #[test]
    fn background_start_close() {
        let dir = std::env::temp_dir().join(format!("oad-bg-{}", std::process::id()));
        let mut b = Background::start(Env::with([
            ("port".to_string(), "0".to_string()),
            ("auth".into(), "none".into()),
            ("dataDir".into(), dir.to_string_lossy().to_string()),
        ]))
        .unwrap();
        assert!(b.port > 0 && b.url.starts_with("http://127.0.0.1:"));
        let s = std::net::TcpStream::connect(("127.0.0.1", b.port));
        assert!(s.is_ok());
        b.close().unwrap();
        b.close().unwrap();
        assert!(std::net::TcpStream::connect(("127.0.0.1", b.port)).is_err());
        let _ = std::fs::remove_dir_all(dir);
        assert!(Background::start(Env::with([("auth".to_string(), "nope".to_string())])).is_err());
    }
}
