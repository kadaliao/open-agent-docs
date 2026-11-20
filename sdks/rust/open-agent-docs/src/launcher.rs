//! 本地启动（契约 §11，feature `launcher`）：在本机拉起一个 `open-agent-docs serve` 子进程。
//!
//! 二进制从哪来，按顺序：[`StartOptions::binary`] → 环境变量 `OAD_BINARY` → 缓存 → 下载
//! （GitHub 发布页的 `open-agent-docs-v<版本>-<target>.tar.gz`，用同一个 release 的 `SHA256SUMS` 验，
//! 解到 `<用户缓存目录>/open-agent-docs/v<版本>/`）。
use crate::error::{Error, Result};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

const RELEASES: &str = "https://github.com/kadaliao/open-agent-docs/releases/download";
const READY_TIMEOUT: Duration = Duration::from_secs(30);
const STOP_TIMEOUT: Duration = Duration::from_secs(10);

/// [`start`] 的参数。除了 `binary`、`port`，别的都转成环境变量 `OAD_<KEY 大写>`（`data_dir` → `OAD_DATA_DIR`），
/// 和命令行一致。
///
/// ```no_run
/// # use open_agent_docs as oad;
/// # async fn run() -> oad::Result<()> {
/// let server = oad::start(
///     oad::StartOptions::new()
///         .data_dir("./data")
///         .auth("header")
///         .header_secret("dev-secret")
///         .set("rate_limit", "0"),
/// )
/// .await?;
/// let admin = oad::Client::builder().base_url(&server.url).header_secret("dev-secret").build()?;
/// # let _ = admin;
/// server.shutdown().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, Default)]
#[must_use]
pub struct StartOptions {
    binary: Option<PathBuf>,
    port: Option<u16>,
    env: Vec<(String, String)>,
}

impl StartOptions {
    pub fn new() -> StartOptions {
        StartOptions::default()
    }

    /// 用这个二进制（不找缓存、不下载）
    pub fn binary(mut self, path: impl Into<PathBuf>) -> Self {
        self.binary = Some(path.into());
        self
    }

    /// 端口（不给就挑一个空的）
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// 数据目录（`OAD_DATA_DIR`）
    pub fn data_dir(self, dir: impl AsRef<Path>) -> Self {
        let d = dir.as_ref().to_string_lossy().into_owned();
        self.set("data_dir", d)
    }

    /// 登录方式（`OAD_AUTH`）：`local` / `header` / `jwt` / `none`
    pub fn auth(self, mode: impl Into<String>) -> Self {
        self.set("auth", mode)
    }

    /// `OAD_HEADER_SECRET`
    pub fn header_secret(self, secret: impl Into<String>) -> Self {
        self.set("header_secret", secret)
    }

    /// `OAD_JWT_SECRET`
    pub fn jwt_secret(self, secret: impl Into<String>) -> Self {
        self.set("jwt_secret", secret)
    }

    /// 任意一项：`set("rate_limit", "0")` → `OAD_RATE_LIMIT=0`
    pub fn set(mut self, key: impl AsRef<str>, value: impl Into<String>) -> Self {
        let k = format!(
            "OAD_{}",
            key.as_ref()
                .trim_start_matches("OAD_")
                .trim_start_matches("oad_")
                .to_uppercase()
        );
        self.env.retain(|(old, _)| *old != k);
        self.env.push((k, value.into()));
        self
    }
}

/// 跑起来的服务器。用完调 [`shutdown`](Server::shutdown)（等页面都存好再返回）；
/// 直接 drop 也会尽力关（发 SIGTERM，不等）。
#[derive(Debug)]
pub struct Server {
    /// `http://127.0.0.1:<port>`
    pub url: String,
    pub port: u16,
    child: Option<Child>,
}

impl Server {
    /// 关掉：SIGTERM，等 10 秒，还没退就 kill。
    pub async fn shutdown(mut self) -> Result<()> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        terminate(&child);
        match tokio::time::timeout(STOP_TIMEOUT, child.wait()).await {
            Ok(r) => r
                .map(|_| ())
                .map_err(|e| Error::Launch(format!("等服务器退出失败：{e}"))),
            Err(_) => {
                let _ = child.kill().await;
                Ok(())
            }
        }
    }

    /// 连这个服务器的客户端配置（`Client::builder().base_url(url)`）
    pub fn client_builder(&self) -> crate::ClientBuilder {
        crate::Client::builder().base_url(&self.url)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(child) = &self.child {
            terminate(child);
        }
    }
}

#[cfg(unix)]
fn terminate(child: &Child) {
    if let Some(pid) = child.id() {
        // SAFETY: 给自己拉起的子进程发信号
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
    }
}

#[cfg(not(unix))]
fn terminate(_child: &Child) {}

/// 在本机拉起服务器，等它能用了再返回。
pub async fn start(opts: StartOptions) -> Result<Server> {
    let binary = match opts.binary.clone() {
        Some(b) => b,
        None => find_binary().await?,
    };
    let port = match opts.port {
        Some(p) => p,
        None => free_port()?,
    };
    let mut cmd = Command::new(&binary);
    cmd.arg("serve")
        .env("OAD_PORT", port.to_string())
        .env("OAD_HOST", "127.0.0.1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false);
    for (k, v) in &opts.env {
        if k != "OAD_PORT" {
            cmd.env(k, v);
        }
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| Error::Launch(format!("跑不了 {}：{e}", binary.display())))?;
    // 输出留最后几十行，起不来时放进错误信息
    let tail = Arc::new(Mutex::new(VecDeque::<String>::new()));
    if let Some(out) = child.stdout.take() {
        tokio::spawn(keep_tail(BufReader::new(out), tail.clone()));
    }
    if let Some(err) = child.stderr.take() {
        tokio::spawn(keep_tail(BufReader::new(err), tail.clone()));
    }
    let url = format!("http://127.0.0.1:{port}");
    let mut server = Server {
        url: url.clone(),
        port,
        child: Some(child),
    };
    let http = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|e| Error::Launch(e.to_string()))?;
    let deadline = Instant::now() + READY_TIMEOUT;
    let tail_text = || {
        tail.lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    };
    loop {
        if let Some(child) = server.child.as_mut() {
            if let Ok(Some(status)) = child.try_wait() {
                server.child = None;
                // 等输出读完
                tokio::time::sleep(Duration::from_millis(100)).await;
                return Err(Error::Launch(format!(
                    "服务器退出了（{status}）：\n{}",
                    tail_text()
                )));
            }
        }
        if let Ok(r) = http.get(format!("{url}/api/v1/auth/config")).send().await {
            if r.status().is_success() {
                return Ok(server);
            }
        }
        if Instant::now() > deadline {
            let _ = server.child.take().map(|mut c| c.start_kill());
            return Err(Error::Launch(format!("30 秒内没有起来：\n{}", tail_text())));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn keep_tail<R: tokio::io::AsyncRead + Unpin>(
    r: BufReader<R>,
    tail: Arc<Mutex<VecDeque<String>>>,
) {
    let mut lines = r.lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let mut t = tail.lock().unwrap_or_else(|e| e.into_inner());
        t.push_back(line);
        while t.len() > 40 {
            t.pop_front();
        }
    }
}

fn free_port() -> Result<u16> {
    let l = std::net::TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| Error::Launch(format!("找不到空端口：{e}")))?;
    l.local_addr()
        .map(|a| a.port())
        .map_err(|e| Error::Launch(e.to_string()))
}

fn target() -> Result<&'static str> {
    let t = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        (os, arch) => {
            return Err(Error::Launch(format!(
                "没有 {os}/{arch} 的预编译二进制：用 Docker（ghcr.io/kadaliao/open-agent-docs），或者自己编好用 OAD_BINARY 指过去"
            )));
        }
    };
    Ok(t)
}

const EXE: &str = if cfg!(windows) {
    "open-agent-docs.exe"
} else {
    "open-agent-docs"
};

async fn find_binary() -> Result<PathBuf> {
    if let Some(b) = std::env::var_os("OAD_BINARY").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(b));
    }
    let version = crate::VERSION;
    let dir = dirs::cache_dir()
        .ok_or_else(|| Error::Launch("找不到用户缓存目录；用 OAD_BINARY 指定二进制".into()))?
        .join("open-agent-docs")
        .join(format!("v{version}"));
    let bin = dir.join(EXE);
    if bin.is_file() {
        return Ok(bin);
    }
    download(version, &dir, &bin).await?;
    Ok(bin)
}

async fn download(version: &str, dir: &Path, bin: &Path) -> Result<()> {
    let target = target()?;
    let name = format!("open-agent-docs-v{version}-{target}.tar.gz");
    let base = format!("{RELEASES}/v{version}");
    let http = reqwest::Client::builder()
        .user_agent(concat!("open-agent-docs-rust/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| Error::Launch(e.to_string()))?;
    let get = |url: String| {
        let http = http.clone();
        async move {
            let r = http
                .get(&url)
                .send()
                .await
                .map_err(|e| Error::Launch(format!("下载 {url}：{e}")))?;
            if !r.status().is_success() {
                return Err(Error::Launch(format!("下载 {url}：{}", r.status())));
            }
            r.bytes()
                .await
                .map_err(|e| Error::Launch(format!("下载 {url}：{e}")))
        }
    };
    let sums = get(format!("{base}/SHA256SUMS")).await?;
    let sums = String::from_utf8_lossy(&sums);
    let want = sums
        .lines()
        .find_map(|l| {
            let mut it = l.split_whitespace();
            let (h, f) = (it.next()?, it.next()?);
            (f.trim_start_matches('*') == name).then(|| h.to_lowercase())
        })
        .ok_or_else(|| Error::Launch(format!("SHA256SUMS 里没有 {name}")))?;
    let tgz = get(format!("{base}/{name}")).await?;
    let got: String = Sha256::digest(&tgz)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if got != want {
        return Err(Error::Launch(format!(
            "{name} 的 SHA256 对不上（{got} ≠ {want}）"
        )));
    }
    let dir = dir.to_path_buf();
    let bin = bin.to_path_buf();
    tokio::task::spawn_blocking(move || extract(&tgz, &dir, &bin))
        .await
        .map_err(|e| Error::Launch(e.to_string()))?
}

/// 从 tar.gz 里取出 `open-agent-docs`，先写临时文件再改名（几个进程同时下载也不会读到半个文件）。
fn extract(tgz: &[u8], dir: &Path, bin: &Path) -> Result<()> {
    let io = |e: std::io::Error| Error::Launch(format!("解压失败：{e}"));
    std::fs::create_dir_all(dir).map_err(io)?;
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(tgz));
    for entry in ar.entries().map_err(io)? {
        let mut entry = entry.map_err(io)?;
        let path = entry.path().map_err(io)?.into_owned();
        if path.file_name().and_then(|n| n.to_str()) != Some(EXE)
            || !entry.header().entry_type().is_file()
        {
            continue;
        }
        let tmp = dir.join(format!(".{EXE}.{}.tmp", std::process::id()));
        let mut f = std::fs::File::create(&tmp).map_err(io)?;
        std::io::copy(&mut entry, &mut f).map_err(io)?;
        drop(f);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).map_err(io)?;
        }
        std::fs::rename(&tmp, bin).map_err(io)?;
        return Ok(());
    }
    Err(Error::Launch(format!("压缩包里没有 {EXE}")))
}
