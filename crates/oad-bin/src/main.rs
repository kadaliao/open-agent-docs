//! open-agent-docs：一个二进制，网页打在里面。
use clap::{Parser, Subcommand, ValueEnum};
use oad_core::s3::{S3Blobs, S3Config};
use oad_core::storage::pg::Postgres;
use oad_core::storage::sqlite::Sqlite;
use oad_core::{
    AuthConfig, BlobStore, Core, FsBlobs, HubConfig, MediaConfig, NotifyConfig, Role, Storage,
};
use oad_server::tenant::{ByHeader, ByHost, TenantResolver};
use oad_server::{AppState, ServerConfig};
use std::io::BufRead;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

#[derive(Parser)]
#[command(name = "open-agent-docs", version, about = "Open Agent Docs 服务器")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// 启动服务器
    Serve(ServeArgs),
    /// 管理内置账号
    User {
        #[command(subcommand)]
        cmd: UserCmd,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum AuthArg {
    Local,
    Header,
    None,
    Jwt,
}

#[derive(Parser)]
struct ServeArgs {
    #[arg(long, env = "OAD_PORT", default_value_t = 8848)]
    port: u16,
    /// 监听地址；对外提供服务时设成 0.0.0.0
    #[arg(long, env = "OAD_HOST", default_value = "127.0.0.1")]
    host: String,
    /// 数据目录：数据库 oad.db（SQLite）、上传的文件、搜索索引。
    /// 设了 OAD_DATABASE_URL=postgres://… 时数据库改用 Postgres，文件和索引仍放这里
    #[arg(long, env = "OAD_DATA_DIR", default_value = "./data")]
    data_dir: PathBuf,
    /// 登录方式；header 模式的密钥从环境变量 OAD_HEADER_SECRET 读；jwt 模式读 OAD_JWT_SECRET（HS256）或 OAD_JWT_JWKS_URL，
    /// 可选 OAD_JWT_ISSUER、OAD_JWT_AUDIENCE、OAD_JWT_LEEWAY（秒，默认 60）
    #[arg(long, env = "OAD_AUTH", value_enum, default_value = "local")]
    auth: AuthArg,
    /// 从这个目录读网页（开发用），不给就用打进二进制的
    #[arg(long, env = "OAD_WEB_DIR")]
    web_dir: Option<PathBuf>,
    /// 会话 cookie 加 Secure（走 HTTPS 时打开）
    #[arg(long, env = "OAD_COOKIE_SECURE")]
    cookie_secure: bool,
    /// 对外的地址，如 https://docs.example.com（它的 Host 和 Origin 都允许）
    #[arg(long, env = "OAD_PUBLIC_URL")]
    public_url: Option<String>,
    /// 额外允许的 Host，逗号分隔；`*` 表示不检查
    #[arg(long, env = "OAD_ALLOWED_HOSTS", value_delimiter = ',')]
    allowed_hosts: Vec<String>,
    /// 允许跨源嵌入的 Origin，逗号分隔，如 https://app.example.com
    #[arg(long, env = "OAD_ALLOWED_ORIGINS", value_delimiter = ',')]
    allowed_origins: Vec<String>,
}

#[derive(Subcommand)]
enum UserCmd {
    /// 加一个用户（密码从环境变量 OAD_USER_PASSWORD 读，没有就从标准输入读一行）
    Add {
        #[arg(long)]
        email: String,
        #[arg(long, default_value = "")]
        name: String,
        /// 在默认工作区的角色
        #[arg(long, value_enum)]
        role: Option<RoleArg>,
        #[arg(long, env = "OAD_DATA_DIR", default_value = "./data")]
        data_dir: PathBuf,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum RoleArg {
    Owner,
    Editor,
    Viewer,
}

impl From<RoleArg> for Role {
    fn from(r: RoleArg) -> Role {
        match r {
            RoleArg::Owner => Role::Owner,
            RoleArg::Editor => Role::Editor,
            RoleArg::Viewer => Role::Viewer,
        }
    }
}

/// 存储：`OAD_DATABASE_URL` 是 postgres:// 就用 Postgres，否则用数据目录里的 oad.db（SQLite）。
/// 数据目录照样放上传的文件和搜索索引。
/// 多租户时每个租户一个 schema（`t_<租户>`）。
async fn open_storage(data_dir: &Path, tenant: Option<&str>) -> Result<Arc<dyn Storage>, String> {
    std::fs::create_dir_all(data_dir).map_err(|e| format!("建数据目录失败：{e}"))?;
    if let Some(url) = env_nonempty("OAD_DATABASE_URL")
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

fn env_nonempty(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

/// 文件存储：`OAD_BLOB_URL=s3://…` 用 S3（多租户时前缀后面加租户名），否则数据目录里的 blobs/。
fn open_blobs(data_dir: &Path, tenant: Option<&str>) -> Result<Arc<dyn BlobStore>, String> {
    if let Some(u) = env_nonempty("OAD_BLOB_URL") {
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
    let storage = open_storage(data_dir, tenant).await?;
    let media = MediaConfig {
        blobs: open_blobs(data_dir, tenant)?,
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
        match (
            env_nonempty("OAD_ADMIN_EMAIL"),
            env_nonempty("OAD_ADMIN_PASSWORD"),
        ) {
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
fn tenant_resolver() -> Result<Option<Box<dyn TenantResolver>>, String> {
    let Some(v) = env_nonempty("OAD_TENANTS") else {
        return Ok(None);
    };
    match v.split_once(':') {
        Some(("host", base)) if !base.is_empty() => Ok(Some(Box::new(ByHost {
            base: base.to_string(),
            default: Some("main".into()),
        }))),
        Some(("header", h)) if !h.is_empty() => Ok(Some(Box::new(ByHeader {
            header: h.to_ascii_lowercase(),
            secret: env_nonempty("OAD_TENANT_SECRET"),
            default: None,
        }))),
        _ => Err("OAD_TENANTS 要是 host:<域名> 或 header:<请求头名>".into()),
    }
}

async fn serve(a: ServeArgs, started: Instant) -> Result<(), String> {
    let auth = match a.auth {
        AuthArg::Local => AuthConfig::Local,
        AuthArg::Header => AuthConfig::Header {
            secret: env_nonempty("OAD_HEADER_SECRET").ok_or("header 模式要设 OAD_HEADER_SECRET")?,
        },
        AuthArg::None => AuthConfig::None,
        AuthArg::Jwt => AuthConfig::Jwt(oad_core::jwt::JwtConfig {
            secret: env_nonempty("OAD_JWT_SECRET"),
            jwks_url: env_nonempty("OAD_JWT_JWKS_URL"),
            issuer: env_nonempty("OAD_JWT_ISSUER"),
            audience: env_nonempty("OAD_JWT_AUDIENCE"),
            leeway: env_nonempty("OAD_JWT_LEEWAY")
                .map(|v| v.parse().map_err(|_| "OAD_JWT_LEEWAY 要是秒数".to_string()))
                .transpose()?
                .unwrap_or(60),
        }),
    };
    let notify = NotifyConfig {
        public_url: a.public_url.clone(),
        allow_private: std::env::var("OAD_WEBHOOK_ALLOW_PRIVATE")
            .is_ok_and(|v| v == "1" || v == "true"),
        ..NotifyConfig::default()
    };
    if notify.allow_private {
        warn!("OAD_WEBHOOK_ALLOW_PRIVATE：webhook 可以投到内网地址（只在本机开发时用）");
    }
    let max_upload_mb: usize = env_nonempty("OAD_MAX_UPLOAD_MB")
        .map(|v| {
            v.parse()
                .map_err(|_| "OAD_MAX_UPLOAD_MB 要是整数".to_string())
        })
        .transpose()?
        .unwrap_or(100);
    if matches!(auth, AuthConfig::None) && a.host != "127.0.0.1" && a.host != "localhost" {
        warn!(host = %a.host, "不登录模式监听在非本机地址上，任何能连上的人都能读写");
    }
    let mode = a.auth_name();
    let mut setup = Setup {
        auth: auth.clone(),
        notify,
        max_upload: max_upload_mb << 20,
        server: ServerConfig {
            web_dir: a.web_dir,
            cookie_secure: a.cookie_secure,
            public_url: a.public_url,
            allowed_hosts: clean(a.allowed_hosts),
            allowed_origins: clean(a.allowed_origins),
            rate_limit: env_nonempty("OAD_RATE_LIMIT")
                .map(|v| {
                    v.parse()
                        .map_err(|_| "OAD_RATE_LIMIT 要是整数（每分钟多少次，0 不限）".to_string())
                })
                .transpose()?
                .unwrap_or(1200),
            ..ServerConfig::default()
        },
    };
    // 单租户：一套；多租户：用到哪个租户建哪个，数据在 <数据目录>/tenants/<租户>/
    let cores: Arc<std::sync::Mutex<Vec<Arc<Core>>>> = Arc::default();
    let app = match tenant_resolver()? {
        None => {
            let core = build_core(&setup, &a.data_dir, None).await?;
            cores.lock().unwrap().push(core.clone());
            oad_server::router(AppState::new(core, setup.server.clone()))
        }
        Some(resolver) => {
            // 租户的 Host 各不一样：Host 检查交给租户解析（认不出的 404）
            setup.server.allowed_hosts = vec!["*".into()];
            let (root, cores2) = (a.data_dir.join("tenants"), cores.clone());
            let factory: oad_server::tenant::TenantFactory = Arc::new(move |t: String| {
                let (setup, root, cores) = (setup.clone(), root.clone(), cores2.clone());
                Box::pin(async move {
                    let core = build_core(&setup, &root.join(&t), Some(&t)).await?;
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
    let addr = format!("{}:{}", a.host, a.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("监听 {addr} 失败：{e}"))?;
    info!(
        "在 http://{addr} 上服务（{} 模式，启动用了 {:.1}ms）",
        mode,
        started.elapsed().as_secs_f64() * 1000.0
    );
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let mut server = tokio::spawn(
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            let _ = stopped.await;
        })
        .into_future(),
    );
    tokio::select! {
        r = &mut server => {
            // 没收到信号服务就停了：出错
            let all: Vec<Arc<Core>> = cores.lock().unwrap().clone();
            for c in all {
                c.shutdown().await;
            }
            return r.map_err(|e| e.to_string())?.map_err(|e| e.to_string());
        }
        _ = shutdown_signal() => {}
    }
    info!("正在关闭，保存打开的页面");
    // 先让 SSE / WebSocket 结束、页面存好，再停 HTTP；都有时限，卡住也能退出
    let all: Vec<Arc<Core>> = cores.lock().unwrap().clone();
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
    let _ = stop.send(());
    match tokio::time::timeout(Duration::from_secs(10), &mut server).await {
        Ok(r) => r.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?,
        Err(_) => warn!("还有连接 10 秒内没结束，强制退出"),
    }
    info!("已关闭");
    Ok(())
}

fn clean(v: Vec<String>) -> Vec<String> {
    v.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

impl ServeArgs {
    fn auth_name(&self) -> &'static str {
        match self.auth {
            AuthArg::Local => "local",
            AuthArg::Header => "header",
            AuthArg::None => "none",
            AuthArg::Jwt => "jwt",
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("注册 SIGTERM 失败");
        tokio::select! {
            _ = ctrl_c => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = ctrl_c.await;
}

async fn user_add(
    email: String,
    name: String,
    role: Option<RoleArg>,
    data_dir: PathBuf,
) -> Result<(), String> {
    let password = match env_nonempty("OAD_USER_PASSWORD") {
        Some(p) => p,
        None => {
            eprintln!("输入密码（至少 8 位），回车结束：");
            let mut line = String::new();
            std::io::stdin()
                .lock()
                .read_line(&mut line)
                .map_err(|e| e.to_string())?;
            line.trim_end_matches(['\r', '\n']).to_string()
        }
    };
    let storage = open_storage(&data_dir, None).await?;
    let core = Core::open(storage, AuthConfig::Local, HubConfig::default())
        .await
        .map_err(|e| e.to_string())?;
    let user = core
        .add_user(&email, &name, &password, role.map(Role::from))
        .await
        .map_err(|e| e.to_string())?;
    println!("已添加用户 {}（{}）", user.name, user.id);
    Ok(())
}

fn main() -> ExitCode {
    let started = Instant::now();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let cli = Cli::parse();
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            error!("启动运行时失败：{e}");
            return ExitCode::FAILURE;
        }
    };
    let r = rt.block_on(async {
        match cli.cmd {
            Cmd::Serve(a) => serve(a, started).await,
            Cmd::User {
                cmd:
                    UserCmd::Add {
                        email,
                        name,
                        role,
                        data_dir,
                    },
            } => user_add(email, name, role, data_dir).await,
        }
    });
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{e}");
            ExitCode::FAILURE
        }
    }
}
