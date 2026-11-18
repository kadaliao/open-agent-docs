//! open-agent-docs：一个二进制，网页打在里面。
use clap::{Parser, Subcommand, ValueEnum};
use oad_core::storage::sqlite::Sqlite;
use oad_core::{AuthConfig, Core, HubConfig, Role};
use oad_server::{AppState, ServerConfig};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;
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
}

#[derive(Parser)]
struct ServeArgs {
    #[arg(long, env = "OAD_PORT", default_value_t = 8848)]
    port: u16,
    /// 监听地址；对外提供服务时设成 0.0.0.0
    #[arg(long, env = "OAD_HOST", default_value = "127.0.0.1")]
    host: String,
    #[arg(long, env = "OAD_DATA_DIR", default_value = "./data")]
    data_dir: PathBuf,
    /// 登录方式；header 模式的密钥从环境变量 OAD_HEADER_SECRET 读
    #[arg(long, env = "OAD_AUTH", value_enum, default_value = "local")]
    auth: AuthArg,
    /// 从这个目录读网页（开发用），不给就用打进二进制的
    #[arg(long, env = "OAD_WEB_DIR")]
    web_dir: Option<PathBuf>,
    /// 会话 cookie 加 Secure（走 HTTPS 时打开）
    #[arg(long, env = "OAD_COOKIE_SECURE")]
    cookie_secure: bool,
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

fn open_storage(data_dir: &Path) -> Result<Arc<Sqlite>, String> {
    std::fs::create_dir_all(data_dir).map_err(|e| format!("建数据目录失败：{e}"))?;
    Sqlite::open(data_dir.join("oad.db"))
        .map(Arc::new)
        .map_err(|e| e.to_string())
}

fn env_nonempty(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

async fn serve(a: ServeArgs, started: Instant) -> Result<(), String> {
    let storage = open_storage(&a.data_dir)?;
    let auth = match a.auth {
        AuthArg::Local => AuthConfig::Local,
        AuthArg::Header => AuthConfig::Header {
            secret: env_nonempty("OAD_HEADER_SECRET").ok_or("header 模式要设 OAD_HEADER_SECRET")?,
        },
        AuthArg::None => AuthConfig::None,
    };
    let core = Core::open(storage, auth.clone(), HubConfig::default())
        .await
        .map_err(|e| e.to_string())?;
    if matches!(auth, AuthConfig::Local) {
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
                    info!(%email, "已建管理员");
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
    if matches!(auth, AuthConfig::None) && a.host != "127.0.0.1" && a.host != "localhost" {
        warn!(host = %a.host, "不登录模式监听在非本机地址上，任何能连上的人都能读写");
    }
    let mode = a.auth_name();
    let state = AppState::new(
        core.clone(),
        ServerConfig {
            web_dir: a.web_dir,
            cookie_secure: a.cookie_secure,
        },
    );
    let app = oad_server::router(state);
    let addr = format!("{}:{}", a.host, a.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("监听 {addr} 失败：{e}"))?;
    info!(
        "在 http://{addr} 上服务（{} 模式，启动用了 {:.1}ms）",
        mode,
        started.elapsed().as_secs_f64() * 1000.0
    );
    let c = core.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            info!("正在关闭，保存打开的页面");
            c.shutdown().await;
        })
        .await
        .map_err(|e| e.to_string())?;
    core.shutdown().await;
    info!("已关闭");
    Ok(())
}

impl ServeArgs {
    fn auth_name(&self) -> &'static str {
        match self.auth {
            AuthArg::Local => "local",
            AuthArg::Header => "header",
            AuthArg::None => "none",
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
    let storage = open_storage(&data_dir)?;
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
