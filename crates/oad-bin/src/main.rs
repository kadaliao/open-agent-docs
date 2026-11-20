//! open-agent-docs：一个二进制，网页打在里面。
use clap::{Parser, Subcommand, ValueEnum};
use oad_bin::Env;
use oad_core::{AuthConfig, Core, HubConfig, Role};
use std::io::BufRead;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;
use tracing::{error, info};

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
    /// 给 Claude Code 当 channel（stdio MCP）：有人在评论里叫这个 Agent 时，把通知推进正在跑的会话
    Channel(ChannelArgs),
}

#[derive(Parser)]
struct ChannelArgs {
    /// 服务器地址，比如 http://127.0.0.1:8848（写 MCP 地址 …/mcp 也行）
    #[arg(long, env = "OAD_URL", default_value = "http://127.0.0.1:8848")]
    url: String,
    /// Agent 的 token（oad_...）；最好用环境变量给，别写在命令行里
    #[arg(long, env = "OAD_TOKEN", hide_env_values = true)]
    token: String,
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

impl ServeArgs {
    fn auth_name(&self) -> &'static str {
        match self.auth {
            AuthArg::Local => "local",
            AuthArg::Header => "header",
            AuthArg::None => "none",
            AuthArg::Jwt => "jwt",
        }
    }

    /// 命令行参数（clap 已经把环境变量合进来了）→ 配置。
    fn env(&self) -> Env {
        let mut env = Env::from_process();
        env.set("OAD_PORT", self.port.to_string());
        env.set("OAD_HOST", self.host.clone());
        env.set("OAD_DATA_DIR", self.data_dir.to_string_lossy());
        env.set("OAD_AUTH", self.auth_name());
        if let Some(w) = &self.web_dir {
            env.set("OAD_WEB_DIR", w.to_string_lossy());
        }
        if self.cookie_secure {
            env.set("OAD_COOKIE_SECURE", "1");
        }
        if let Some(u) = &self.public_url {
            env.set("OAD_PUBLIC_URL", u.clone());
        }
        if !self.allowed_hosts.is_empty() {
            env.set("OAD_ALLOWED_HOSTS", self.allowed_hosts.join(","));
        }
        if !self.allowed_origins.is_empty() {
            env.set("OAD_ALLOWED_ORIGINS", self.allowed_origins.join(","));
        }
        env
    }
}

async fn serve(a: ServeArgs, started: Instant) -> Result<(), String> {
    let mut running = oad_bin::start(&a.env()).await?;
    info!("启动用了 {:.1}ms", started.elapsed().as_secs_f64() * 1000.0);
    tokio::select! {
        r = running.stopped() => {
            // 没收到信号服务就停了：出错
            return r;
        }
        _ = shutdown_signal() => {}
    }
    info!("正在关闭，保存打开的页面");
    running.shutdown().await?;
    info!("已关闭");
    Ok(())
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
    let password = match Env::from_process().get("OAD_USER_PASSWORD") {
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
    let storage = oad_bin::open_storage(&Env::from_process(), &data_dir, None).await?;
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
    let cli = Cli::parse();
    // channel 的 stdout 是 MCP 协议，日志只能写 stderr
    if matches!(cli.cmd, Cmd::Channel(_)) {
        oad_bin::init_logging_stderr();
    } else {
        oad_bin::init_logging();
    }
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
            Cmd::Channel(a) => {
                oad_bin::channel::run(oad_bin::channel::Opts {
                    url: a.url,
                    token: a.token,
                })
                .await
            }
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
