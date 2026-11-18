//! 通知：Agent 的（webhook / SSE / 长轮询三种取法）和人的（收件箱）。
//!
//! - Agent 的通知存表，`id` 是游标。新通知来了 `wake` 加一，SSE 和长轮询看到就再查一次库。
//! - webhook 由一个后台任务投：到时间的取出来投，失败按 `retry` 的间隔再投，都失败了记下错误不再投。
//!   签名：`X-OAD-Signature: sha256=<hex>`，HMAC-SHA256(webhook_secret, "<毫秒时间戳>.<body>")。
//!   只投公网地址（[`crate::ssrf`]），`allow_private` 时允许内网。
//! - 超过 `retention` 的通知定期删掉。
use crate::events::{Event, Events};
use crate::ids::now_ms;
use crate::model::*;
use crate::storage::Storage;
use crate::{Result, ssrf};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Notify, watch};
use tracing::{debug, warn};

#[derive(Clone, Debug)]
pub struct NotifyConfig {
    /// 网页地址（通知里的页面链接用），如 `https://docs.example.com`。
    pub public_url: Option<String>,
    /// 允许 webhook 投到内网（本机开发、测试）。
    pub allow_private: bool,
    /// 第一次失败后每次重试前等多久；用完了就放弃。
    pub retry: Vec<Duration>,
    pub timeout: Duration,
    pub retention: Duration,
}

impl Default for NotifyConfig {
    fn default() -> Self {
        NotifyConfig {
            public_url: None,
            allow_private: false,
            retry: vec![
                Duration::from_secs(10),
                Duration::from_secs(60),
                Duration::from_secs(300),
                Duration::from_secs(1800),
            ],
            timeout: Duration::from_secs(10),
            retention: Duration::from_secs(30 * 24 * 3600),
        }
    }
}

pub struct Notifier {
    storage: Arc<dyn Storage>,
    events: Events,
    pub cfg: NotifyConfig,
    /// 最新一条 Agent 通知的编号。
    wake: watch::Sender<i64>,
    kick: Notify,
    http: reqwest::Client,
}

/// webhook 签名：HMAC-SHA256(secret, "<timestamp>.<body>") 的十六进制。
pub fn sign(secret: &str, timestamp: i64, body: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC 接受任意长度的密钥");
    mac.update(format!("{timestamp}.{body}").as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 发给 Agent 的一条通知（加上编号和时间）。
pub fn agent_payload(row: &NotificationRow) -> Value {
    let mut v = row.data.clone();
    if let Some(o) = v.as_object_mut() {
        o.insert("id".into(), row.id.into());
        o.insert("created_at".into(), row.created_at.into());
    }
    v
}

/// 收件箱里的一条。
pub fn user_payload(row: &NotificationRow) -> Value {
    json!({
        "id": row.id,
        "workspace_id": row.workspace_id,
        "type": row.kind,
        "data": row.data,
        "created_at": row.created_at,
        "read_at": row.done_at,
    })
}

impl Notifier {
    pub fn new(storage: Arc<dyn Storage>, events: Events, cfg: NotifyConfig) -> Arc<Notifier> {
        let mut b = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(cfg.timeout)
            .user_agent("open-agent-docs-webhook");
        if !cfg.allow_private {
            b = b.dns_resolver(Arc::new(ssrf::GuardedResolver));
        }
        Arc::new(Notifier {
            storage,
            events,
            http: b.build().expect("建 HTTP 客户端失败"),
            cfg,
            wake: watch::channel(0).0,
            kick: Notify::new(),
        })
    }

    /// 网页里页面的地址。
    pub fn page_url(&self, page_id: &str) -> String {
        let base = self
            .cfg
            .public_url
            .as_deref()
            .unwrap_or("")
            .trim_end_matches('/');
        format!("{base}/p/{page_id}")
    }

    /// 设 webhook 时先查一遍地址（域名要等投的时候解析了才知道）。
    pub fn check_webhook(&self, url: &str) -> Result<()> {
        let u = url::Url::parse(url)
            .map_err(|e| crate::Error::BadRequest(format!("webhook 地址不对：{e}")))?;
        if !matches!(u.scheme(), "http" | "https") {
            return Err(crate::Error::BadRequest("webhook 只能是 http(s)".into()));
        }
        if !self.cfg.allow_private {
            ssrf::check_url(&u)
                .map_err(|e| crate::Error::BadRequest(format!("webhook 地址不允许：{e}")))?;
        }
        Ok(())
    }

    /// 新通知订阅（SSE、长轮询用）：值是最新的通知编号。
    pub fn subscribe(&self) -> watch::Receiver<i64> {
        self.wake.subscribe()
    }

    /// 存 Agent 通知，叫醒等着的。
    pub async fn push_agent(&self, rows: Vec<NewNotification>) -> Result<Vec<NotificationRow>> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let hook = rows.iter().any(|r| r.next_attempt_at.is_some());
        let saved = self.storage.add_agent_notifications(rows).await?;
        if let Some(last) = saved.iter().map(|r| r.id).max() {
            self.wake.send_modify(|v| *v = (*v).max(last));
        }
        if hook {
            self.kick.notify_one();
        }
        Ok(saved)
    }

    /// 存人的通知，经工作区 SSE 推给本人。
    pub async fn push_user(&self, rows: Vec<NewNotification>) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        for r in self.storage.add_user_notifications(rows).await? {
            self.events.emit(Event {
                workspace_id: r.workspace_id.clone(),
                name: "notification",
                data: user_payload(&r),
                user_id: Some(r.owner_id.clone()),
            });
        }
        Ok(())
    }

    /// 有新的要投（建通知时已经调过，测试用）。
    pub fn kick(&self) {
        self.kick.notify_one();
    }

    /// 投一条；返回 Err(原因) 表示失败。
    async fn deliver(&self, row: &NotificationRow) -> std::result::Result<(), String> {
        let agent = self
            .storage
            .agent(&row.owner_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("Agent 不存在")?;
        if !agent.active() {
            return Err("Agent 已撤销".into());
        }
        let url = agent.webhook.as_deref().ok_or("没有设 webhook")?;
        let u = url::Url::parse(url).map_err(|e| e.to_string())?;
        if !self.cfg.allow_private {
            // IP 写在网址里时不经过 DNS 解析，这里查
            ssrf::check_url(&u)?;
        }
        let body = agent_payload(row).to_string();
        let ts = now_ms();
        let res = self
            .http
            .post(u)
            .header("content-type", "application/json")
            .header("x-oad-timestamp", ts.to_string())
            .header(
                "x-oad-signature",
                format!("sha256={}", sign(&agent.webhook_secret, ts, &body)),
            )
            .header("x-oad-delivery", row.id.to_string())
            .body(body)
            .send()
            .await
            .map_err(|e| {
                // 带上底层原因（比如被 SSRF 检查拦下）
                let mut msg = e.to_string();
                let mut src = std::error::Error::source(&e);
                while let Some(s) = src {
                    msg = format!("{msg}：{s}");
                    src = s.source();
                }
                msg
            })?;
        if res.status().is_success() {
            Ok(())
        } else {
            Err(format!("HTTP {}", res.status().as_u16()))
        }
    }

    async fn run_due(&self) -> Result<()> {
        loop {
            let due = self.storage.due_webhooks(now_ms(), 20).await?;
            if due.is_empty() {
                return Ok(());
            }
            for row in due {
                match self.deliver(&row).await {
                    Ok(()) => {
                        debug!(id = row.id, "webhook 送到");
                        self.storage
                            .record_delivery(row.id, true, None, None, now_ms())
                            .await?;
                    }
                    Err(e) => {
                        // attempts 是这次之前投过几次；第 n 次失败后等 retry[n-1]
                        let next = self
                            .cfg
                            .retry
                            .get(row.attempts as usize)
                            .map(|d| now_ms() + d.as_millis() as i64);
                        warn!(
                            id = row.id,
                            "webhook 投递失败（第 {} 次）：{e}",
                            row.attempts + 1
                        );
                        self.storage
                            .record_delivery(row.id, false, Some(e), next, now_ms())
                            .await?;
                    }
                }
            }
        }
    }

    /// 后台任务：投 webhook、清旧通知，直到 stop。
    pub async fn run(self: Arc<Self>, mut stop: impl Future<Output = ()> + Unpin) {
        let mut last_purge = 0i64;
        loop {
            if let Err(e) = self.run_due().await {
                warn!("投 webhook 出错：{e}");
            }
            let now = now_ms();
            if now - last_purge > 3600 * 1000 {
                last_purge = now;
                let before = now - self.cfg.retention.as_millis() as i64;
                match self.storage.purge_notifications(before).await {
                    Ok(n) if n > 0 => debug!("删了 {n} 条过期通知"),
                    Ok(_) => {}
                    Err(e) => warn!("清理通知出错：{e}"),
                }
            }
            let wait = match self.storage.next_webhook_at().await {
                Ok(Some(t)) => Duration::from_millis((t - now_ms()).clamp(0, 60_000) as u64),
                _ => Duration::from_secs(60),
            };
            tokio::select! {
                _ = &mut stop => return,
                _ = self.kick.notified() => {}
                _ = tokio::time::sleep(wait) => {}
            }
        }
    }
}

impl crate::Core {
    /// 调用者能收哪些 Agent 通知：自己的；admin 收整个工作区的。
    async fn note_scope(&self, who: &Principal) -> Result<NoteScope> {
        let a = self
            .agent_of(who)
            .await?
            .filter(Agent::active)
            .ok_or_else(|| crate::Error::Forbidden("只有 Agent 能收 Agent 通知".into()))?;
        Ok(match a.scope {
            Scope::Admin => NoteScope::Workspace(a.workspace_id),
            _ => NoteScope::Agent(a.id),
        })
    }

    /// after 之后的通知；不给 after 就是还没处理的。
    pub async fn agent_notes(
        &self,
        who: &Principal,
        after: Option<i64>,
        limit: i64,
    ) -> Result<Vec<Value>> {
        let scope = self.note_scope(who).await?;
        Ok(self
            .storage
            .agent_notifications(scope, after, limit.clamp(1, 100))
            .await?
            .iter()
            .map(agent_payload)
            .collect())
    }

    /// 长轮询：没有就等，最多 wait。
    pub async fn wait_agent_notes(
        &self,
        who: &Principal,
        after: Option<i64>,
        wait: Duration,
        limit: i64,
    ) -> Result<Vec<Value>> {
        let mut rx = self.notes.subscribe();
        let deadline = tokio::time::Instant::now() + wait.min(Duration::from_secs(60));
        let mut stop = std::pin::pin!(self.on_shutdown());
        loop {
            rx.borrow_and_update();
            let got = self.agent_notes(who, after, limit).await?;
            if !got.is_empty() {
                return Ok(got);
            }
            tokio::select! {
                r = tokio::time::timeout_at(deadline, rx.changed()) => {
                    if !matches!(r, Ok(Ok(()))) {
                        return Ok(Vec::new());
                    }
                }
                _ = &mut stop => return Ok(Vec::new()),
            }
        }
    }

    /// 处理完了：ids 里的，或者编号 ≤ upto 的。返回标了几条。
    pub async fn ack_agent_notes(
        &self,
        who: &Principal,
        ids: Vec<i64>,
        upto: Option<i64>,
    ) -> Result<u64> {
        let scope = self.note_scope(who).await?;
        self.storage
            .ack_agent_notifications(scope, ids, upto, now_ms())
            .await
    }

    /// 收件箱，新的在前；after 往旧的翻。
    pub async fn user_notes(
        &self,
        who: &Principal,
        after: Option<i64>,
        limit: i64,
    ) -> Result<Vec<Value>> {
        Ok(self
            .storage
            .user_notifications(&who.user_id, after, limit.clamp(1, 100))
            .await?
            .iter()
            .map(user_payload)
            .collect())
    }

    /// 标成已读：ids 里的，或者 None = 全部。
    pub async fn read_user_notes(&self, who: &Principal, ids: Option<Vec<i64>>) -> Result<u64> {
        self.storage
            .read_user_notifications(&who.user_id, ids, now_ms())
            .await
    }
}
