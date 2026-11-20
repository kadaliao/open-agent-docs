// 生成的，不要手改：改 docs/openapi.json 或 sdks/operations.json，再跑 `cargo run -p codegen`（在 sdks/rust 下）。
//! 资源和接口：`client.pages().versions().list(id)`。每个接口一个请求构造器：
//! 位置参数是路径参数（和必填的查询参数）、请求体；可选的查询参数用链式方法设；`.await` 发出去。
#![allow(
    rustdoc::broken_intra_doc_links,
    rustdoc::invalid_html_tags,
    rustdoc::bare_urls,
    clippy::doc_lazy_continuation
)]

use crate::Client;
use crate::error::Result;
use crate::request::{Spec, enc};
use crate::types::*;
use futures_util::future::BoxFuture;
use std::future::IntoFuture;

/// `search` 的请求：`GET /api/v1/workspaces/{ws}/search`；返回响应里的 `results`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct SearchRequest {
    client: Client,
    ws: String,
    q: String,
    limit: Option<u64>,
    kind: Option<String>,
    per: Option<String>,
}

impl SearchRequest {
    /// 最多几条，默认 20，最多 100（查询参数 `limit`）
    pub fn limit(mut self, v: u64) -> Self {
        self.limit = Some(v);
        self
    }

    /// page / row / all（默认）（查询参数 `kind`）
    pub fn kind(mut self, v: impl Into<String>) -> Self {
        self.kind = Some(v.into());
        self
    }

    /// `block`：每块一条（同一页最多两条）；默认每页一条（查询参数 `per`）
    pub fn per(mut self, v: impl Into<String>) -> Self {
        self.per = Some(v.into());
        self
    }

    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/workspaces/{}/search", enc(&self.ws)),
        );
        spec.query("q", self.q.to_string());
        if let Some(v) = &self.limit {
            spec.query("limit", v.to_string());
        }
        if let Some(v) = &self.kind {
            spec.query("kind", v.to_string());
        }
        if let Some(v) = &self.per {
            spec.query("per", v.to_string());
        }
        Ok(spec)
    }
}

impl IntoFuture for SearchRequest {
    type Output = Result<Vec<SearchResult>>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move {
            let r: SearchResults = client.send_json(spec?).await?;
            Ok(r.results)
        })
    }
}

impl Client {
    /// `agent.*` 的接口
    pub fn agent(&self) -> AgentApi {
        AgentApi {
            client: self.clone(),
        }
    }

    /// `agents.*` 的接口
    pub fn agents(&self) -> AgentsApi {
        AgentsApi {
            client: self.clone(),
        }
    }

    /// `auth.*` 的接口
    pub fn auth(&self) -> AuthApi {
        AuthApi {
            client: self.clone(),
        }
    }

    /// `comments.*` 的接口
    pub fn comments(&self) -> CommentsApi {
        CommentsApi {
            client: self.clone(),
        }
    }

    /// `databases.*` 的接口
    pub fn databases(&self) -> DatabasesApi {
        DatabasesApi {
            client: self.clone(),
        }
    }

    /// `files.*` 的接口
    pub fn files(&self) -> FilesApi {
        FilesApi {
            client: self.clone(),
        }
    }

    /// `invites.*` 的接口
    pub fn invites(&self) -> InvitesApi {
        InvitesApi {
            client: self.clone(),
        }
    }

    /// `links.*` 的接口
    pub fn links(&self) -> LinksApi {
        LinksApi {
            client: self.clone(),
        }
    }

    /// `me.*` 的接口
    pub fn me(&self) -> MeApi {
        MeApi {
            client: self.clone(),
        }
    }

    /// `pages.*` 的接口
    pub fn pages(&self) -> PagesApi {
        PagesApi {
            client: self.clone(),
        }
    }

    /// `threads.*` 的接口
    pub fn threads(&self) -> ThreadsApi {
        ThreadsApi {
            client: self.clone(),
        }
    }

    /// `trash.*` 的接口
    pub fn trash(&self) -> TrashApi {
        TrashApi {
            client: self.clone(),
        }
    }

    /// `workspaces.*` 的接口
    pub fn workspaces(&self) -> WorkspacesApi {
        WorkspacesApi {
            client: self.clone(),
        }
    }

    /// `GET /api/v1/workspaces/{ws}/search`；返回响应里的 `results`
    pub fn search(&self, ws: impl Into<String>, q: impl Into<String>) -> SearchRequest {
        SearchRequest {
            client: self.clone(),
            ws: ws.into(),
            q: q.into(),
            limit: None,
            kind: None,
            per: None,
        }
    }
}

/// `agent.*` 的接口。
#[derive(Clone, Debug)]
pub struct AgentApi {
    pub(crate) client: Client,
}

impl AgentApi {
    /// `agent.notifications.*` 的接口
    pub fn notifications(&self) -> AgentNotificationsApi {
        AgentNotificationsApi {
            client: self.client.clone(),
        }
    }
}

/// `agent.notifications.*` 的接口。
#[derive(Clone, Debug)]
pub struct AgentNotificationsApi {
    pub(crate) client: Client,
}

/// `agent.notifications.list` 的请求：Agent 取通知（长轮询）：没有新的就等 wait 秒。不给 after / cursor 就是还没处理（ack）的。只有 Agent 能调。
///
/// `GET /api/v1/agent/notifications`。一页；逐条取完所有页用 `list_all` 或 `.into_stream()`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct AgentNotificationsListRequest {
    client: Client,
    after: Option<i64>,
    cursor: Option<String>,
    limit: Option<i64>,
    wait: Option<i64>,
}

impl crate::Paginated for AgentNotificationsListRequest {
    type Item = AgentNotification;

    fn cursor(&self) -> Option<String> {
        self.cursor.clone()
    }

    fn with_cursor(&self, cursor: Option<String>) -> Self {
        let mut r = self.clone();
        r.cursor = cursor;
        r
    }

    fn fetch(self) -> crate::pagination::PageFuture<AgentNotification> {
        Box::pin(async move {
            let p: AgentNotes = self.await?;
            Ok((p.notifications, p.next_cursor))
        })
    }
}

impl crate::ListAll<AgentNotificationsListRequest> {
    /// 从这个编号之后取
    pub fn after(self, v: i64) -> Self {
        self.map_request(|r| r.after(v))
    }

    /// 同 after（上次的 next_cursor）
    pub fn cursor(self, v: impl Into<String>) -> Self {
        self.map_request(|r| r.cursor(v.into()))
    }

    /// 最多几条，默认 50，最多 100
    pub fn limit(self, v: i64) -> Self {
        self.map_request(|r| r.limit(v))
    }

    /// 没有就等几秒，默认 0，最多 60
    pub fn wait(self, v: i64) -> Self {
        self.map_request(|r| r.wait(v))
    }
}

impl AgentNotificationsListRequest {
    /// 从这个编号之后取（查询参数 `after`）
    pub fn after(mut self, v: i64) -> Self {
        self.after = Some(v);
        self
    }

    /// 同 after（上次的 next_cursor）（查询参数 `cursor`）
    pub fn cursor(mut self, v: impl Into<String>) -> Self {
        self.cursor = Some(v.into());
        self
    }

    /// 最多几条，默认 50，最多 100（查询参数 `limit`）
    pub fn limit(mut self, v: i64) -> Self {
        self.limit = Some(v);
        self
    }

    /// 没有就等几秒，默认 0，最多 60（查询参数 `wait`）
    pub fn wait(mut self, v: i64) -> Self {
        self.wait = Some(v);
        self
    }

    /// 逐条取完所有页（自动带 `cursor`）。
    pub fn into_stream(self) -> crate::ListAll<Self> {
        crate::ListAll::new(self)
    }

    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(reqwest::Method::GET, "/agent/notifications".to_string());
        if let Some(v) = &self.after {
            spec.query("after", v.to_string());
        }
        if let Some(v) = &self.cursor {
            spec.query("cursor", v.to_string());
        }
        if let Some(v) = &self.limit {
            spec.query("limit", v.to_string());
        }
        if let Some(v) = &self.wait {
            spec.query("wait", v.to_string());
        }
        Ok(spec)
    }
}

impl IntoFuture for AgentNotificationsListRequest {
    type Output = Result<AgentNotes>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `agent.notifications.ack` 的请求：`POST /api/v1/agent/notifications/ack`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct AgentNotificationsAckRequest {
    client: Client,
    body: Ack,
}

impl AgentNotificationsAckRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            "/agent/notifications/ack".to_string(),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for AgentNotificationsAckRequest {
    type Output = Result<Acked>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `agent.notifications.stream` 的请求：SSE：先补发游标之后的（`?after=` / `?cursor=` 或 `Last-Event-ID` 头；都没有就是没处理的），再实时推。每条是 `event: notification`，`id` 是通知编号，`data` 是 AgentNotification。每 25 秒一个注释行保活。只有 Agent 能调。
///
/// `GET /api/v1/agent/notifications/stream`；`.await` 连上以后得到事件流，断了自动重连（带 `Last-Event-ID`）
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct AgentNotificationsStreamRequest {
    client: Client,
    after: Option<i64>,
    cursor: Option<String>,
}

impl AgentNotificationsStreamRequest {
    /// 从这个编号之后（查询参数 `after`）
    pub fn after(mut self, v: i64) -> Self {
        self.after = Some(v);
        self
    }

    /// 同 after（查询参数 `cursor`）
    pub fn cursor(mut self, v: impl Into<String>) -> Self {
        self.cursor = Some(v.into());
        self
    }

    fn spec(&self, last_event_id: Option<&str>) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            "/agent/notifications/stream".to_string(),
        );
        if let Some(id) = last_event_id {
            spec.header("last-event-id", id);
        }
        match last_event_id {
            Some(id) => spec.query("after", id.to_string()),
            None => {
                if let Some(v) = &self.after {
                    spec.query("after", v.to_string());
                }
            }
        }
        if let (None, Some(v)) = (last_event_id, &self.cursor) {
            spec.query("cursor", v.to_string());
        }
        Ok(spec)
    }
}

impl IntoFuture for AgentNotificationsStreamRequest {
    type Output = Result<crate::events::EventStream<AgentNotification>>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let client = self.client.clone();
            client
                .open_events::<AgentNotification>(std::sync::Arc::new(move |last: Option<&str>| {
                    self.spec(last)
                }))
                .await
        })
    }
}

impl AgentNotificationsApi {
    /// Agent 取通知（长轮询）：没有新的就等 wait 秒。不给 after / cursor 就是还没处理（ack）的。只有 Agent 能调。
    ///
    /// `GET /api/v1/agent/notifications`。一页；逐条取完所有页用 `list_all` 或 `.into_stream()`
    pub fn list(&self) -> AgentNotificationsListRequest {
        AgentNotificationsListRequest {
            client: self.client.clone(),
            after: None,
            cursor: None,
            limit: None,
            wait: None,
        }
    }

    /// 和 [`list`](Self::list) 一样，但逐条取完所有页：返回 `Stream<Item = Result<条目>>`，停止条件是没有下一页、这页是空的或游标没变。
    pub fn list_all(&self) -> crate::ListAll<AgentNotificationsListRequest> {
        self.list().into_stream()
    }

    /// `POST /api/v1/agent/notifications/ack`
    pub fn ack(&self, body: Ack) -> AgentNotificationsAckRequest {
        AgentNotificationsAckRequest {
            client: self.client.clone(),
            body,
        }
    }

    /// SSE：先补发游标之后的（`?after=` / `?cursor=` 或 `Last-Event-ID` 头；都没有就是没处理的），再实时推。每条是 `event: notification`，`id` 是通知编号，`data` 是 AgentNotification。每 25 秒一个注释行保活。只有 Agent 能调。
    ///
    /// `GET /api/v1/agent/notifications/stream`；`.await` 连上以后得到事件流，断了自动重连（带 `Last-Event-ID`）
    pub fn stream(&self) -> AgentNotificationsStreamRequest {
        AgentNotificationsStreamRequest {
            client: self.client.clone(),
            after: None,
            cursor: None,
        }
    }
}

/// `agents.*` 的接口。
#[derive(Clone, Debug)]
pub struct AgentsApi {
    pub(crate) client: Client,
}

/// `agents.list` 的请求：`GET /api/v1/workspaces/{ws}/agents`；返回响应里的 `agents`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct AgentsListRequest {
    client: Client,
    ws: String,
}

impl AgentsListRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/workspaces/{}/agents", enc(&self.ws)),
        );
        Ok(spec)
    }
}

impl IntoFuture for AgentsListRequest {
    type Output = Result<Vec<AgentView>>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move {
            let r: AgentList = client.send_json(spec?).await?;
            Ok(r.agents)
        })
    }
}

/// `agents.create` 的请求：`POST /api/v1/workspaces/{ws}/agents`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct AgentsCreateRequest {
    client: Client,
    ws: String,
    body: CreateAgent,
}

impl AgentsCreateRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/workspaces/{}/agents", enc(&self.ws)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for AgentsCreateRequest {
    type Output = Result<CreatedAgent>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `agents.update` 的请求：`PATCH /api/v1/agents/{id}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct AgentsUpdateRequest {
    client: Client,
    id: String,
    body: PatchAgent,
}

impl AgentsUpdateRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(reqwest::Method::PATCH, format!("/agents/{}", enc(&self.id)));
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for AgentsUpdateRequest {
    type Output = Result<Agent>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `agents.revoke` 的请求：撤销：token 马上失效，返回撤销后的 Agent（带 revoked_at）。
///
/// `DELETE /api/v1/agents/{id}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct AgentsRevokeRequest {
    client: Client,
    id: String,
}

impl AgentsRevokeRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::DELETE,
            format!("/agents/{}", enc(&self.id)),
        );
        Ok(spec)
    }
}

impl IntoFuture for AgentsRevokeRequest {
    type Output = Result<Agent>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `agents.rotate_token` 的请求：`POST /api/v1/agents/{id}/token`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct AgentsRotateTokenRequest {
    client: Client,
    id: String,
}

impl AgentsRotateTokenRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/agents/{}/token", enc(&self.id)),
        );
        Ok(spec)
    }
}

impl IntoFuture for AgentsRotateTokenRequest {
    type Output = Result<NewToken>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `agents.rotate_webhook_secret` 的请求：`POST /api/v1/agents/{id}/webhook-secret`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct AgentsRotateWebhookSecretRequest {
    client: Client,
    id: String,
}

impl AgentsRotateWebhookSecretRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/agents/{}/webhook-secret", enc(&self.id)),
        );
        Ok(spec)
    }
}

impl IntoFuture for AgentsRotateWebhookSecretRequest {
    type Output = Result<NewSecret>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

impl AgentsApi {
    /// `GET /api/v1/workspaces/{ws}/agents`；返回响应里的 `agents`
    pub fn list(&self, ws: impl Into<String>) -> AgentsListRequest {
        AgentsListRequest {
            client: self.client.clone(),
            ws: ws.into(),
        }
    }

    /// `POST /api/v1/workspaces/{ws}/agents`
    pub fn create(&self, ws: impl Into<String>, body: CreateAgent) -> AgentsCreateRequest {
        AgentsCreateRequest {
            client: self.client.clone(),
            ws: ws.into(),
            body,
        }
    }

    /// `PATCH /api/v1/agents/{id}`
    pub fn update(&self, id: impl Into<String>, body: PatchAgent) -> AgentsUpdateRequest {
        AgentsUpdateRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }

    /// 撤销：token 马上失效，返回撤销后的 Agent（带 revoked_at）。
    ///
    /// `DELETE /api/v1/agents/{id}`
    pub fn revoke(&self, id: impl Into<String>) -> AgentsRevokeRequest {
        AgentsRevokeRequest {
            client: self.client.clone(),
            id: id.into(),
        }
    }

    /// `POST /api/v1/agents/{id}/token`
    pub fn rotate_token(&self, id: impl Into<String>) -> AgentsRotateTokenRequest {
        AgentsRotateTokenRequest {
            client: self.client.clone(),
            id: id.into(),
        }
    }

    /// `POST /api/v1/agents/{id}/webhook-secret`
    pub fn rotate_webhook_secret(&self, id: impl Into<String>) -> AgentsRotateWebhookSecretRequest {
        AgentsRotateWebhookSecretRequest {
            client: self.client.clone(),
            id: id.into(),
        }
    }
}

/// `auth.*` 的接口。
#[derive(Clone, Debug)]
pub struct AuthApi {
    pub(crate) client: Client,
}

/// `auth.config` 的请求：`GET /api/v1/auth/config`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct AuthConfigRequest {
    client: Client,
}

impl AuthConfigRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(reqwest::Method::GET, "/auth/config".to_string());
        Ok(spec)
    }
}

impl IntoFuture for AuthConfigRequest {
    type Output = Result<AuthConfigBody>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `auth.login` 的请求：`POST /api/v1/auth/login`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct AuthLoginRequest {
    client: Client,
    body: LoginBody,
}

impl AuthLoginRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(reqwest::Method::POST, "/auth/login".to_string());
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for AuthLoginRequest {
    type Output = Result<Me>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `auth.logout` 的请求：`POST /api/v1/auth/logout`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct AuthLogoutRequest {
    client: Client,
}

impl AuthLogoutRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(reqwest::Method::POST, "/auth/logout".to_string());
        Ok(spec)
    }
}

impl IntoFuture for AuthLogoutRequest {
    type Output = Result<()>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_empty(spec?).await })
    }
}

/// `auth.signup` 的请求：注册（本地登录模式）。OAD_SIGNUP：invite（默认，要凭邀请）、open、off。
///
/// `POST /api/v1/auth/signup`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct AuthSignupRequest {
    client: Client,
    body: SignupBody,
}

impl AuthSignupRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(reqwest::Method::POST, "/auth/signup".to_string());
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for AuthSignupRequest {
    type Output = Result<Me>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

impl AuthApi {
    /// `GET /api/v1/auth/config`
    pub fn config(&self) -> AuthConfigRequest {
        AuthConfigRequest {
            client: self.client.clone(),
        }
    }

    /// `POST /api/v1/auth/login`
    pub fn login(&self, body: LoginBody) -> AuthLoginRequest {
        AuthLoginRequest {
            client: self.client.clone(),
            body,
        }
    }

    /// `POST /api/v1/auth/logout`
    pub fn logout(&self) -> AuthLogoutRequest {
        AuthLogoutRequest {
            client: self.client.clone(),
        }
    }

    /// 注册（本地登录模式）。OAD_SIGNUP：invite（默认，要凭邀请）、open、off。
    ///
    /// `POST /api/v1/auth/signup`
    pub fn signup(&self, body: SignupBody) -> AuthSignupRequest {
        AuthSignupRequest {
            client: self.client.clone(),
            body,
        }
    }
}

/// `comments.*` 的接口。
#[derive(Clone, Debug)]
pub struct CommentsApi {
    pub(crate) client: Client,
}

/// `comments.update` 的请求：只有作者能改。
///
/// `PATCH /api/v1/comments/{id}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct CommentsUpdateRequest {
    client: Client,
    id: String,
    body: EditComment,
}

impl CommentsUpdateRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::PATCH,
            format!("/comments/{}", enc(&self.id)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for CommentsUpdateRequest {
    type Output = Result<CommentView>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `comments.delete` 的请求：`DELETE /api/v1/comments/{id}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct CommentsDeleteRequest {
    client: Client,
    id: String,
}

impl CommentsDeleteRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::DELETE,
            format!("/comments/{}", enc(&self.id)),
        );
        Ok(spec)
    }
}

impl IntoFuture for CommentsDeleteRequest {
    type Output = Result<()>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_empty(spec?).await })
    }
}

impl CommentsApi {
    /// 只有作者能改。
    ///
    /// `PATCH /api/v1/comments/{id}`
    pub fn update(&self, id: impl Into<String>, body: EditComment) -> CommentsUpdateRequest {
        CommentsUpdateRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }

    /// `DELETE /api/v1/comments/{id}`
    pub fn delete(&self, id: impl Into<String>) -> CommentsDeleteRequest {
        CommentsDeleteRequest {
            client: self.client.clone(),
            id: id.into(),
        }
    }
}

/// `databases.*` 的接口。
#[derive(Clone, Debug)]
pub struct DatabasesApi {
    pub(crate) client: Client,
}

/// `databases.create` 的请求：`POST /api/v1/workspaces/{ws}/databases`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesCreateRequest {
    client: Client,
    ws: String,
    body: NewDb,
}

impl DatabasesCreateRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/workspaces/{}/databases", enc(&self.ws)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for DatabasesCreateRequest {
    type Output = Result<DbInfo>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `databases.import_csv` 的请求：`POST /api/v1/workspaces/{ws}/databases/import`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesImportCsvRequest {
    client: Client,
    ws: String,
    title: Option<String>,
    parent_id: Option<String>,
    data: bytes::Bytes,
}

impl DatabasesImportCsvRequest {
    /// 库名（multipart 里的 title 优先；都没有用文件名）（查询参数 `title`）
    pub fn title(mut self, v: impl Into<String>) -> Self {
        self.title = Some(v.into());
        self
    }

    /// 放在哪个页面下（不给放在根上）（查询参数 `parent_id`）
    pub fn parent_id(mut self, v: impl Into<String>) -> Self {
        self.parent_id = Some(v.into());
        self
    }

    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/workspaces/{}/databases/import", enc(&self.ws)),
        );
        if let Some(v) = &self.title {
            spec.query("title", v.to_string());
        }
        if let Some(v) = &self.parent_id {
            spec.query("parent_id", v.to_string());
        }
        spec.raw("text/csv", self.data.clone());
        Ok(spec)
    }
}

impl IntoFuture for DatabasesImportCsvRequest {
    type Output = Result<DbInfo>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `databases.get` 的请求：`GET /api/v1/databases/{id}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesGetRequest {
    client: Client,
    id: String,
}

impl DatabasesGetRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/databases/{}", enc(&self.id)),
        );
        Ok(spec)
    }
}

impl IntoFuture for DatabasesGetRequest {
    type Output = Result<DbInfo>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `databases.query` 的请求：`POST /api/v1/databases/{id}/query`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesQueryRequest {
    client: Client,
    id: String,
    body: QueryReq,
}

impl DatabasesQueryRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/databases/{}/query", enc(&self.id)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for DatabasesQueryRequest {
    type Output = Result<QueryResult>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `databases.export_csv` 的请求：`GET /api/v1/databases/{id}/export.csv`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesExportCsvRequest {
    client: Client,
    id: String,
}

impl DatabasesExportCsvRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/databases/{}/export.csv", enc(&self.id)),
        );
        Ok(spec)
    }
}

impl IntoFuture for DatabasesExportCsvRequest {
    type Output = Result<bytes::Bytes>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_bytes(spec?).await })
    }
}

/// `databases.press_button` 的请求：`POST /api/v1/databases/{id}/button/{row}/{key}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesPressButtonRequest {
    client: Client,
    id: String,
    row: String,
    key: String,
}

impl DatabasesPressButtonRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!(
                "/databases/{}/button/{}/{}",
                enc(&self.id),
                enc(&self.row),
                enc(&self.key)
            ),
        );
        Ok(spec)
    }
}

impl IntoFuture for DatabasesPressButtonRequest {
    type Output = Result<Pressed>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

impl DatabasesApi {
    /// `databases.history.*` 的接口
    pub fn history(&self) -> DatabasesHistoryApi {
        DatabasesHistoryApi {
            client: self.client.clone(),
        }
    }

    /// `databases.rows.*` 的接口
    pub fn rows(&self) -> DatabasesRowsApi {
        DatabasesRowsApi {
            client: self.client.clone(),
        }
    }

    /// `databases.schema.*` 的接口
    pub fn schema(&self) -> DatabasesSchemaApi {
        DatabasesSchemaApi {
            client: self.client.clone(),
        }
    }

    /// `databases.views.*` 的接口
    pub fn views(&self) -> DatabasesViewsApi {
        DatabasesViewsApi {
            client: self.client.clone(),
        }
    }

    /// `POST /api/v1/workspaces/{ws}/databases`
    pub fn create(&self, ws: impl Into<String>, body: NewDb) -> DatabasesCreateRequest {
        DatabasesCreateRequest {
            client: self.client.clone(),
            ws: ws.into(),
            body,
        }
    }

    /// `POST /api/v1/workspaces/{ws}/databases/import`
    pub fn import_csv(
        &self,
        ws: impl Into<String>,
        data: impl Into<bytes::Bytes>,
    ) -> DatabasesImportCsvRequest {
        DatabasesImportCsvRequest {
            client: self.client.clone(),
            ws: ws.into(),
            title: None,
            parent_id: None,
            data: data.into(),
        }
    }

    /// `GET /api/v1/databases/{id}`
    pub fn get(&self, id: impl Into<String>) -> DatabasesGetRequest {
        DatabasesGetRequest {
            client: self.client.clone(),
            id: id.into(),
        }
    }

    /// `POST /api/v1/databases/{id}/query`
    pub fn query(&self, id: impl Into<String>, body: QueryReq) -> DatabasesQueryRequest {
        DatabasesQueryRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }

    /// `GET /api/v1/databases/{id}/export.csv`
    pub fn export_csv(&self, id: impl Into<String>) -> DatabasesExportCsvRequest {
        DatabasesExportCsvRequest {
            client: self.client.clone(),
            id: id.into(),
        }
    }

    /// `POST /api/v1/databases/{id}/button/{row}/{key}`
    pub fn press_button(
        &self,
        id: impl Into<String>,
        row: impl Into<String>,
        key: impl Into<String>,
    ) -> DatabasesPressButtonRequest {
        DatabasesPressButtonRequest {
            client: self.client.clone(),
            id: id.into(),
            row: row.into(),
            key: key.into(),
        }
    }
}

/// `databases.history.*` 的接口。
#[derive(Clone, Debug)]
pub struct DatabasesHistoryApi {
    pub(crate) client: Client,
}

/// `databases.history.list` 的请求：`GET /api/v1/databases/{id}/history`。一页；逐条取完所有页用 `list_all` 或 `.into_stream()`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesHistoryListRequest {
    client: Client,
    id: String,
    row: Option<String>,
    prop: Option<String>,
    limit: Option<u64>,
    cursor: Option<String>,
}

impl crate::Paginated for DatabasesHistoryListRequest {
    type Item = DbHistoryItem;

    fn cursor(&self) -> Option<String> {
        self.cursor.clone()
    }

    fn with_cursor(&self, cursor: Option<String>) -> Self {
        let mut r = self.clone();
        r.cursor = cursor;
        r
    }

    fn fetch(self) -> crate::pagination::PageFuture<DbHistoryItem> {
        Box::pin(async move {
            let p: DbHistory = self.await?;
            Ok((p.items, p.next_cursor))
        })
    }
}

impl crate::ListAll<DatabasesHistoryListRequest> {
    /// 只看这一行
    pub fn row(self, v: impl Into<String>) -> Self {
        self.map_request(|r| r.row(v.into()))
    }

    /// 只看这个属性（key 或名字）
    pub fn prop(self, v: impl Into<String>) -> Self {
        self.map_request(|r| r.prop(v.into()))
    }

    /// 一页几条，默认 200，最多 1000
    pub fn limit(self, v: u64) -> Self {
        self.map_request(|r| r.limit(v))
    }

    /// 上一页给的 `next_cursor`
    pub fn cursor(self, v: impl Into<String>) -> Self {
        self.map_request(|r| r.cursor(v.into()))
    }
}

impl DatabasesHistoryListRequest {
    /// 只看这一行（查询参数 `row`）
    pub fn row(mut self, v: impl Into<String>) -> Self {
        self.row = Some(v.into());
        self
    }

    /// 只看这个属性（key 或名字）（查询参数 `prop`）
    pub fn prop(mut self, v: impl Into<String>) -> Self {
        self.prop = Some(v.into());
        self
    }

    /// 一页几条，默认 200，最多 1000（查询参数 `limit`）
    pub fn limit(mut self, v: u64) -> Self {
        self.limit = Some(v);
        self
    }

    /// 上一页给的 `next_cursor`（查询参数 `cursor`）
    pub fn cursor(mut self, v: impl Into<String>) -> Self {
        self.cursor = Some(v.into());
        self
    }

    /// 逐条取完所有页（自动带 `cursor`）。
    pub fn into_stream(self) -> crate::ListAll<Self> {
        crate::ListAll::new(self)
    }

    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/databases/{}/history", enc(&self.id)),
        );
        if let Some(v) = &self.row {
            spec.query("row", v.to_string());
        }
        if let Some(v) = &self.prop {
            spec.query("prop", v.to_string());
        }
        if let Some(v) = &self.limit {
            spec.query("limit", v.to_string());
        }
        if let Some(v) = &self.cursor {
            spec.query("cursor", v.to_string());
        }
        Ok(spec)
    }
}

impl IntoFuture for DatabasesHistoryListRequest {
    type Output = Result<DbHistory>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `databases.history.undo` 的请求：`POST /api/v1/databases/{id}/history/undo`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesHistoryUndoRequest {
    client: Client,
    id: String,
    body: UndoBody,
}

impl DatabasesHistoryUndoRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/databases/{}/history/undo", enc(&self.id)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for DatabasesHistoryUndoRequest {
    type Output = Result<DbUndone>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

impl DatabasesHistoryApi {
    /// `GET /api/v1/databases/{id}/history`。一页；逐条取完所有页用 `list_all` 或 `.into_stream()`
    pub fn list(&self, id: impl Into<String>) -> DatabasesHistoryListRequest {
        DatabasesHistoryListRequest {
            client: self.client.clone(),
            id: id.into(),
            row: None,
            prop: None,
            limit: None,
            cursor: None,
        }
    }

    /// 和 [`list`](Self::list) 一样，但逐条取完所有页：返回 `Stream<Item = Result<条目>>`，停止条件是没有下一页、这页是空的或游标没变。
    pub fn list_all(&self, id: impl Into<String>) -> crate::ListAll<DatabasesHistoryListRequest> {
        self.list(id).into_stream()
    }

    /// `POST /api/v1/databases/{id}/history/undo`
    pub fn undo(&self, id: impl Into<String>, body: UndoBody) -> DatabasesHistoryUndoRequest {
        DatabasesHistoryUndoRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }
}

/// `databases.rows.*` 的接口。
#[derive(Clone, Debug)]
pub struct DatabasesRowsApi {
    pub(crate) client: Client,
}

/// `databases.rows.list` 的请求：`GET /api/v1/databases/{id}/rows`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesRowsListRequest {
    client: Client,
    id: String,
    since: Option<i64>,
}

impl DatabasesRowsListRequest {
    /// 只要这个 seq 之后改了的行（和删了的行 id）；不给是全量（查询参数 `since`）
    pub fn since(mut self, v: i64) -> Self {
        self.since = Some(v);
        self
    }

    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/databases/{}/rows", enc(&self.id)),
        );
        if let Some(v) = &self.since {
            spec.query("since", v.to_string());
        }
        Ok(spec)
    }
}

impl IntoFuture for DatabasesRowsListRequest {
    type Output = Result<RowsDelta>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `databases.rows.add` 的请求：`POST /api/v1/databases/{id}/rows`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesRowsAddRequest {
    client: Client,
    id: String,
    body: AddRows,
}

impl DatabasesRowsAddRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/databases/{}/rows", enc(&self.id)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for DatabasesRowsAddRequest {
    type Output = Result<AddedRows>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `databases.rows.update` 的请求：`PATCH /api/v1/databases/{id}/rows`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesRowsUpdateRequest {
    client: Client,
    id: String,
    body: PatchRows,
}

impl DatabasesRowsUpdateRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::PATCH,
            format!("/databases/{}/rows", enc(&self.id)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for DatabasesRowsUpdateRequest {
    type Output = Result<Patched>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `databases.rows.delete` 的请求：`DELETE /api/v1/databases/{id}/rows`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesRowsDeleteRequest {
    client: Client,
    id: String,
    body: Ids,
}

impl DatabasesRowsDeleteRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::DELETE,
            format!("/databases/{}/rows", enc(&self.id)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for DatabasesRowsDeleteRequest {
    type Output = Result<Seq>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `databases.rows.delete_one` 的请求：`DELETE /api/v1/databases/{id}/rows/{row}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesRowsDeleteOneRequest {
    client: Client,
    id: String,
    row: String,
}

impl DatabasesRowsDeleteOneRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::DELETE,
            format!("/databases/{}/rows/{}", enc(&self.id), enc(&self.row)),
        );
        Ok(spec)
    }
}

impl IntoFuture for DatabasesRowsDeleteOneRequest {
    type Output = Result<Seq>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

impl DatabasesRowsApi {
    /// `GET /api/v1/databases/{id}/rows`
    pub fn list(&self, id: impl Into<String>) -> DatabasesRowsListRequest {
        DatabasesRowsListRequest {
            client: self.client.clone(),
            id: id.into(),
            since: None,
        }
    }

    /// `POST /api/v1/databases/{id}/rows`
    pub fn add(&self, id: impl Into<String>, body: AddRows) -> DatabasesRowsAddRequest {
        DatabasesRowsAddRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }

    /// `PATCH /api/v1/databases/{id}/rows`
    pub fn update(&self, id: impl Into<String>, body: PatchRows) -> DatabasesRowsUpdateRequest {
        DatabasesRowsUpdateRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }

    /// `DELETE /api/v1/databases/{id}/rows`
    pub fn delete(&self, id: impl Into<String>, body: Ids) -> DatabasesRowsDeleteRequest {
        DatabasesRowsDeleteRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }

    /// `DELETE /api/v1/databases/{id}/rows/{row}`
    pub fn delete_one(
        &self,
        id: impl Into<String>,
        row: impl Into<String>,
    ) -> DatabasesRowsDeleteOneRequest {
        DatabasesRowsDeleteOneRequest {
            client: self.client.clone(),
            id: id.into(),
            row: row.into(),
        }
    }
}

/// `databases.schema.*` 的接口。
#[derive(Clone, Debug)]
pub struct DatabasesSchemaApi {
    pub(crate) client: Client,
}

/// `databases.schema.update` 的请求：`PATCH /api/v1/databases/{id}/schema`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesSchemaUpdateRequest {
    client: Client,
    id: String,
    body: SchemaOps,
}

impl DatabasesSchemaUpdateRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::PATCH,
            format!("/databases/{}/schema", enc(&self.id)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for DatabasesSchemaUpdateRequest {
    type Output = Result<SchemaChanged>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `databases.schema.undo` 的请求：`POST /api/v1/databases/{id}/schema/undo`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesSchemaUndoRequest {
    client: Client,
    id: String,
    body: Undo,
}

impl DatabasesSchemaUndoRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/databases/{}/schema/undo", enc(&self.id)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for DatabasesSchemaUndoRequest {
    type Output = Result<SchemaUndone>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

impl DatabasesSchemaApi {
    /// `PATCH /api/v1/databases/{id}/schema`
    pub fn update(&self, id: impl Into<String>, body: SchemaOps) -> DatabasesSchemaUpdateRequest {
        DatabasesSchemaUpdateRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }

    /// `POST /api/v1/databases/{id}/schema/undo`
    pub fn undo(&self, id: impl Into<String>, body: Undo) -> DatabasesSchemaUndoRequest {
        DatabasesSchemaUndoRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }
}

/// `databases.views.*` 的接口。
#[derive(Clone, Debug)]
pub struct DatabasesViewsApi {
    pub(crate) client: Client,
}

/// `databases.views.create` 的请求：`POST /api/v1/databases/{id}/views`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesViewsCreateRequest {
    client: Client,
    id: String,
    body: serde_json::Value,
}

impl DatabasesViewsCreateRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/databases/{}/views", enc(&self.id)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for DatabasesViewsCreateRequest {
    type Output = Result<DbView>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `databases.views.replace` 的请求：`PUT /api/v1/databases/{id}/views`；返回响应里的 `views`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesViewsReplaceRequest {
    client: Client,
    id: String,
    body: PutViews,
}

impl DatabasesViewsReplaceRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::PUT,
            format!("/databases/{}/views", enc(&self.id)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for DatabasesViewsReplaceRequest {
    type Output = Result<Vec<DbView>>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move {
            let r: Views = client.send_json(spec?).await?;
            Ok(r.views)
        })
    }
}

/// `databases.views.update` 的请求：`PATCH /api/v1/databases/{id}/views/{view}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesViewsUpdateRequest {
    client: Client,
    id: String,
    view: String,
    body: serde_json::Value,
}

impl DatabasesViewsUpdateRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::PATCH,
            format!("/databases/{}/views/{}", enc(&self.id), enc(&self.view)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for DatabasesViewsUpdateRequest {
    type Output = Result<DbView>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `databases.views.delete` 的请求：`DELETE /api/v1/databases/{id}/views/{view}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct DatabasesViewsDeleteRequest {
    client: Client,
    id: String,
    view: String,
}

impl DatabasesViewsDeleteRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::DELETE,
            format!("/databases/{}/views/{}", enc(&self.id), enc(&self.view)),
        );
        Ok(spec)
    }
}

impl IntoFuture for DatabasesViewsDeleteRequest {
    type Output = Result<()>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_empty(spec?).await })
    }
}

impl DatabasesViewsApi {
    /// `POST /api/v1/databases/{id}/views`
    pub fn create(
        &self,
        id: impl Into<String>,
        body: serde_json::Value,
    ) -> DatabasesViewsCreateRequest {
        DatabasesViewsCreateRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }

    /// `PUT /api/v1/databases/{id}/views`；返回响应里的 `views`
    pub fn replace(&self, id: impl Into<String>, body: PutViews) -> DatabasesViewsReplaceRequest {
        DatabasesViewsReplaceRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }

    /// `PATCH /api/v1/databases/{id}/views/{view}`
    pub fn update(
        &self,
        id: impl Into<String>,
        view: impl Into<String>,
        body: serde_json::Value,
    ) -> DatabasesViewsUpdateRequest {
        DatabasesViewsUpdateRequest {
            client: self.client.clone(),
            id: id.into(),
            view: view.into(),
            body,
        }
    }

    /// `DELETE /api/v1/databases/{id}/views/{view}`
    pub fn delete(
        &self,
        id: impl Into<String>,
        view: impl Into<String>,
    ) -> DatabasesViewsDeleteRequest {
        DatabasesViewsDeleteRequest {
            client: self.client.clone(),
            id: id.into(),
            view: view.into(),
        }
    }
}

/// `files.*` 的接口。
#[derive(Clone, Debug)]
pub struct FilesApi {
    pub(crate) client: Client,
}

/// `files.upload` 的请求：上传文件：`multipart/form-data` 的 `file` 字段，或者原始正文 + `X-Filename` 头（文件名按网址规则编码）。大小上限见 OAD_MAX_UPLOAD。图片会算宽高和 blurhash，音视频算时长。
///
/// `POST /api/v1/workspaces/{ws}/files`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct FilesUploadRequest {
    client: Client,
    ws: String,
    filename: String,
    data: bytes::Bytes,
}

impl FilesUploadRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/workspaces/{}/files", enc(&self.ws)),
        );
        spec.header("x-filename", enc(&self.filename));
        spec.raw("application/octet-stream", self.data.clone());
        Ok(spec)
    }
}

impl IntoFuture for FilesUploadRequest {
    type Output = Result<Uploaded>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `files.download` 的请求：同 `/files/{id}/{name}`，不带文件名。
///
/// `GET /api/v1/files/{id}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct FilesDownloadRequest {
    client: Client,
    id: String,
    w: Option<u32>,
    range: Option<String>,
}

impl FilesDownloadRequest {
    /// 缩略图宽度（图片才有，给 WebP）（查询参数 `w`）
    pub fn w(mut self, v: u32) -> Self {
        self.w = Some(v);
        self
    }

    /// `bytes=a-b`，只认一段（请求头参数 `Range`）
    pub fn range(mut self, v: impl Into<String>) -> Self {
        self.range = Some(v.into());
        self
    }

    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(reqwest::Method::GET, format!("/files/{}", enc(&self.id)));
        if let Some(v) = &self.w {
            spec.query("w", v.to_string());
        }
        if let Some(v) = &self.range {
            spec.header("Range", v.to_string());
        }
        Ok(spec)
    }
}

impl IntoFuture for FilesDownloadRequest {
    type Output = Result<bytes::Bytes>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_bytes(spec?).await })
    }
}

/// `files.download_named` 的请求：下载上传的文件：原来的类型；带 ETag、长缓存、Range。`?w=` 给图片的缩略图（WebP）。SVG、HTML 之类带 `Content-Security-Policy: sandbox`，SVG 当附件下载。不是这个工作区的成员当不存在（404）。
///
/// `GET /api/v1/files/{id}/{name}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct FilesDownloadNamedRequest {
    client: Client,
    id: String,
    name: String,
    w: Option<u32>,
    range: Option<String>,
}

impl FilesDownloadNamedRequest {
    /// 缩略图宽度（图片才有，给 WebP）（查询参数 `w`）
    pub fn w(mut self, v: u32) -> Self {
        self.w = Some(v);
        self
    }

    /// `bytes=a-b`，只认一段（请求头参数 `Range`）
    pub fn range(mut self, v: impl Into<String>) -> Self {
        self.range = Some(v.into());
        self
    }

    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/files/{}/{}", enc(&self.id), enc(&self.name)),
        );
        if let Some(v) = &self.w {
            spec.query("w", v.to_string());
        }
        if let Some(v) = &self.range {
            spec.header("Range", v.to_string());
        }
        Ok(spec)
    }
}

impl IntoFuture for FilesDownloadNamedRequest {
    type Output = Result<bytes::Bytes>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_bytes(spec?).await })
    }
}

/// `files.media` 的请求：转存的外部图片（链接卡片的封面、头像之类），按内容的 hash。登录了就能看。`?w=` 给缩略图（WebP），支持 Range。
///
/// `GET /api/v1/media/{hash}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct FilesMediaRequest {
    client: Client,
    hash: String,
    w: Option<u32>,
    range: Option<String>,
}

impl FilesMediaRequest {
    /// 缩略图宽度（查询参数 `w`）
    pub fn w(mut self, v: u32) -> Self {
        self.w = Some(v);
        self
    }

    /// `bytes=a-b`，只认一段（请求头参数 `Range`）
    pub fn range(mut self, v: impl Into<String>) -> Self {
        self.range = Some(v.into());
        self
    }

    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(reqwest::Method::GET, format!("/media/{}", enc(&self.hash)));
        if let Some(v) = &self.w {
            spec.query("w", v.to_string());
        }
        if let Some(v) = &self.range {
            spec.header("Range", v.to_string());
        }
        Ok(spec)
    }
}

impl IntoFuture for FilesMediaRequest {
    type Output = Result<bytes::Bytes>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_bytes(spec?).await })
    }
}

impl FilesApi {
    /// 上传文件：`multipart/form-data` 的 `file` 字段，或者原始正文 + `X-Filename` 头（文件名按网址规则编码）。大小上限见 OAD_MAX_UPLOAD。图片会算宽高和 blurhash，音视频算时长。
    ///
    /// `POST /api/v1/workspaces/{ws}/files`
    pub fn upload(
        &self,
        ws: impl Into<String>,
        filename: impl Into<String>,
        data: impl Into<bytes::Bytes>,
    ) -> FilesUploadRequest {
        FilesUploadRequest {
            client: self.client.clone(),
            ws: ws.into(),
            filename: filename.into(),
            data: data.into(),
        }
    }

    /// 同 `/files/{id}/{name}`，不带文件名。
    ///
    /// `GET /api/v1/files/{id}`
    pub fn download(&self, id: impl Into<String>) -> FilesDownloadRequest {
        FilesDownloadRequest {
            client: self.client.clone(),
            id: id.into(),
            w: None,
            range: None,
        }
    }

    /// 下载上传的文件：原来的类型；带 ETag、长缓存、Range。`?w=` 给图片的缩略图（WebP）。SVG、HTML 之类带 `Content-Security-Policy: sandbox`，SVG 当附件下载。不是这个工作区的成员当不存在（404）。
    ///
    /// `GET /api/v1/files/{id}/{name}`
    pub fn download_named(
        &self,
        id: impl Into<String>,
        name: impl Into<String>,
    ) -> FilesDownloadNamedRequest {
        FilesDownloadNamedRequest {
            client: self.client.clone(),
            id: id.into(),
            name: name.into(),
            w: None,
            range: None,
        }
    }

    /// 转存的外部图片（链接卡片的封面、头像之类），按内容的 hash。登录了就能看。`?w=` 给缩略图（WebP），支持 Range。
    ///
    /// `GET /api/v1/media/{hash}`
    pub fn media(&self, hash: impl Into<String>) -> FilesMediaRequest {
        FilesMediaRequest {
            client: self.client.clone(),
            hash: hash.into(),
            w: None,
            range: None,
        }
    }
}

/// `invites.*` 的接口。
#[derive(Clone, Debug)]
pub struct InvitesApi {
    pub(crate) client: Client,
}

/// `invites.get` 的请求：打开邀请链接时看的：哪个工作区、什么角色、谁邀请的。不用登录。
///
/// `GET /api/v1/invites/{token}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct InvitesGetRequest {
    client: Client,
    token: String,
}

impl InvitesGetRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/invites/{}", enc(&self.token)),
        );
        Ok(spec)
    }
}

impl IntoFuture for InvitesGetRequest {
    type Output = Result<InviteInfo>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `invites.accept` 的请求：接受邀请，加入工作区（已经在里面的不降角色）。返回加入后的 /me。
///
/// `POST /api/v1/invites/{token}/accept`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct InvitesAcceptRequest {
    client: Client,
    token: String,
}

impl InvitesAcceptRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/invites/{}/accept", enc(&self.token)),
        );
        Ok(spec)
    }
}

impl IntoFuture for InvitesAcceptRequest {
    type Output = Result<Me>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `invites.list` 的请求：`GET /api/v1/workspaces/{ws}/invites`；返回响应里的 `invites`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct InvitesListRequest {
    client: Client,
    ws: String,
}

impl InvitesListRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/workspaces/{}/invites", enc(&self.ws)),
        );
        Ok(spec)
    }
}

impl IntoFuture for InvitesListRequest {
    type Output = Result<Vec<InviteView>>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move {
            let r: InviteList = client.send_json(spec?).await?;
            Ok(r.invites)
        })
    }
}

/// `invites.create` 的请求：`POST /api/v1/workspaces/{ws}/invites`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct InvitesCreateRequest {
    client: Client,
    ws: String,
    body: NewInvite,
}

impl InvitesCreateRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/workspaces/{}/invites", enc(&self.ws)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for InvitesCreateRequest {
    type Output = Result<InviteView>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `invites.revoke` 的请求：`DELETE /api/v1/workspaces/{ws}/invites/{token}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct InvitesRevokeRequest {
    client: Client,
    ws: String,
    token: String,
}

impl InvitesRevokeRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::DELETE,
            format!("/workspaces/{}/invites/{}", enc(&self.ws), enc(&self.token)),
        );
        Ok(spec)
    }
}

impl IntoFuture for InvitesRevokeRequest {
    type Output = Result<()>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_empty(spec?).await })
    }
}

impl InvitesApi {
    /// 打开邀请链接时看的：哪个工作区、什么角色、谁邀请的。不用登录。
    ///
    /// `GET /api/v1/invites/{token}`
    pub fn get(&self, token: impl Into<String>) -> InvitesGetRequest {
        InvitesGetRequest {
            client: self.client.clone(),
            token: token.into(),
        }
    }

    /// 接受邀请，加入工作区（已经在里面的不降角色）。返回加入后的 /me。
    ///
    /// `POST /api/v1/invites/{token}/accept`
    pub fn accept(&self, token: impl Into<String>) -> InvitesAcceptRequest {
        InvitesAcceptRequest {
            client: self.client.clone(),
            token: token.into(),
        }
    }

    /// `GET /api/v1/workspaces/{ws}/invites`；返回响应里的 `invites`
    pub fn list(&self, ws: impl Into<String>) -> InvitesListRequest {
        InvitesListRequest {
            client: self.client.clone(),
            ws: ws.into(),
        }
    }

    /// `POST /api/v1/workspaces/{ws}/invites`
    pub fn create(&self, ws: impl Into<String>, body: NewInvite) -> InvitesCreateRequest {
        InvitesCreateRequest {
            client: self.client.clone(),
            ws: ws.into(),
            body,
        }
    }

    /// `DELETE /api/v1/workspaces/{ws}/invites/{token}`
    pub fn revoke(&self, ws: impl Into<String>, token: impl Into<String>) -> InvitesRevokeRequest {
        InvitesRevokeRequest {
            client: self.client.clone(),
            ws: ws.into(),
            token: token.into(),
        }
    }
}

/// `links.*` 的接口。
#[derive(Clone, Debug)]
pub struct LinksApi {
    pub(crate) client: Client,
}

/// `links.unfurl` 的请求：解析一个网址成卡片：上传的文件直接从库里给，本站的库给视图列表，别的网址去抓（有缓存，图片转存到 /media）。
///
/// `GET /api/v1/unfurl`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct LinksUnfurlRequest {
    client: Client,
    url: String,
}

impl LinksUnfurlRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(reqwest::Method::GET, "/unfurl".to_string());
        spec.query("url", self.url.to_string());
        Ok(spec)
    }
}

impl IntoFuture for LinksUnfurlRequest {
    type Output = Result<UnfurlCard>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

impl LinksApi {
    /// 解析一个网址成卡片：上传的文件直接从库里给，本站的库给视图列表，别的网址去抓（有缓存，图片转存到 /media）。
    ///
    /// `GET /api/v1/unfurl`
    pub fn unfurl(&self, url: impl Into<String>) -> LinksUnfurlRequest {
        LinksUnfurlRequest {
            client: self.client.clone(),
            url: url.into(),
        }
    }
}

/// `me.*` 的接口。
#[derive(Clone, Debug)]
pub struct MeApi {
    pub(crate) client: Client,
}

/// `me.get` 的请求：`GET /api/v1/me`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct MeGetRequest {
    client: Client,
}

impl MeGetRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(reqwest::Method::GET, "/me".to_string());
        Ok(spec)
    }
}

impl IntoFuture for MeGetRequest {
    type Output = Result<Me>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `me.update` 的请求：`PATCH /api/v1/me`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct MeUpdateRequest {
    client: Client,
    body: MePatch,
}

impl MeUpdateRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(reqwest::Method::PATCH, "/me".to_string());
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for MeUpdateRequest {
    type Output = Result<Me>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `me.change_password` 的请求：改密码（本地登录）。别的会话都退出，这次请求用的会话保留。
///
/// `POST /api/v1/me/password`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct MeChangePasswordRequest {
    client: Client,
    body: PasswordBody,
}

impl MeChangePasswordRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(reqwest::Method::POST, "/me/password".to_string());
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for MeChangePasswordRequest {
    type Output = Result<()>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_empty(spec?).await })
    }
}

impl MeApi {
    /// `me.notifications.*` 的接口
    pub fn notifications(&self) -> MeNotificationsApi {
        MeNotificationsApi {
            client: self.client.clone(),
        }
    }

    /// `GET /api/v1/me`
    pub fn get(&self) -> MeGetRequest {
        MeGetRequest {
            client: self.client.clone(),
        }
    }

    /// `PATCH /api/v1/me`
    pub fn update(&self, body: MePatch) -> MeUpdateRequest {
        MeUpdateRequest {
            client: self.client.clone(),
            body,
        }
    }

    /// 改密码（本地登录）。别的会话都退出，这次请求用的会话保留。
    ///
    /// `POST /api/v1/me/password`
    pub fn change_password(&self, body: PasswordBody) -> MeChangePasswordRequest {
        MeChangePasswordRequest {
            client: self.client.clone(),
            body,
        }
    }
}

/// `me.notifications.*` 的接口。
#[derive(Clone, Debug)]
pub struct MeNotificationsApi {
    pub(crate) client: Client,
}

/// `me.notifications.list` 的请求：`GET /api/v1/me/notifications`。一页；逐条取完所有页用 `list_all` 或 `.into_stream()`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct MeNotificationsListRequest {
    client: Client,
    after: Option<i64>,
    cursor: Option<String>,
    limit: Option<i64>,
}

impl crate::Paginated for MeNotificationsListRequest {
    type Item = InboxNotification;

    fn cursor(&self) -> Option<String> {
        self.cursor.clone()
    }

    fn with_cursor(&self, cursor: Option<String>) -> Self {
        let mut r = self.clone();
        r.cursor = cursor;
        r
    }

    fn fetch(self) -> crate::pagination::PageFuture<InboxNotification> {
        Box::pin(async move {
            let p: Inbox = self.await?;
            Ok((p.notifications, p.next_cursor))
        })
    }
}

impl crate::ListAll<MeNotificationsListRequest> {
    /// 从这个编号往旧的翻
    pub fn after(self, v: i64) -> Self {
        self.map_request(|r| r.after(v))
    }

    /// 同 after（上次的 next_cursor）
    pub fn cursor(self, v: impl Into<String>) -> Self {
        self.map_request(|r| r.cursor(v.into()))
    }

    /// 最多几条，默认 50，最多 100
    pub fn limit(self, v: i64) -> Self {
        self.map_request(|r| r.limit(v))
    }
}

impl MeNotificationsListRequest {
    /// 从这个编号往旧的翻（查询参数 `after`）
    pub fn after(mut self, v: i64) -> Self {
        self.after = Some(v);
        self
    }

    /// 同 after（上次的 next_cursor）（查询参数 `cursor`）
    pub fn cursor(mut self, v: impl Into<String>) -> Self {
        self.cursor = Some(v.into());
        self
    }

    /// 最多几条，默认 50，最多 100（查询参数 `limit`）
    pub fn limit(mut self, v: i64) -> Self {
        self.limit = Some(v);
        self
    }

    /// 逐条取完所有页（自动带 `cursor`）。
    pub fn into_stream(self) -> crate::ListAll<Self> {
        crate::ListAll::new(self)
    }

    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(reqwest::Method::GET, "/me/notifications".to_string());
        if let Some(v) = &self.after {
            spec.query("after", v.to_string());
        }
        if let Some(v) = &self.cursor {
            spec.query("cursor", v.to_string());
        }
        if let Some(v) = &self.limit {
            spec.query("limit", v.to_string());
        }
        Ok(spec)
    }
}

impl IntoFuture for MeNotificationsListRequest {
    type Output = Result<Inbox>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `me.notifications.mark_read` 的请求：`POST /api/v1/me/notifications/read`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct MeNotificationsMarkReadRequest {
    client: Client,
    body: ReadNotes,
}

impl MeNotificationsMarkReadRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(reqwest::Method::POST, "/me/notifications/read".to_string());
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for MeNotificationsMarkReadRequest {
    type Output = Result<ReadCount>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

impl MeNotificationsApi {
    /// `GET /api/v1/me/notifications`。一页；逐条取完所有页用 `list_all` 或 `.into_stream()`
    pub fn list(&self) -> MeNotificationsListRequest {
        MeNotificationsListRequest {
            client: self.client.clone(),
            after: None,
            cursor: None,
            limit: None,
        }
    }

    /// 和 [`list`](Self::list) 一样，但逐条取完所有页：返回 `Stream<Item = Result<条目>>`，停止条件是没有下一页、这页是空的或游标没变。
    pub fn list_all(&self) -> crate::ListAll<MeNotificationsListRequest> {
        self.list().into_stream()
    }

    /// `POST /api/v1/me/notifications/read`
    pub fn mark_read(&self, body: ReadNotes) -> MeNotificationsMarkReadRequest {
        MeNotificationsMarkReadRequest {
            client: self.client.clone(),
            body,
        }
    }
}

/// `pages.*` 的接口。
#[derive(Clone, Debug)]
pub struct PagesApi {
    pub(crate) client: Client,
}

/// `pages.list` 的请求：`GET /api/v1/workspaces/{ws}/pages`；返回响应里的 `pages`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesListRequest {
    client: Client,
    ws: String,
}

impl PagesListRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/workspaces/{}/pages", enc(&self.ws)),
        );
        Ok(spec)
    }
}

impl IntoFuture for PagesListRequest {
    type Output = Result<Vec<Page>>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move {
            let r: PageList = client.send_json(spec?).await?;
            Ok(r.pages)
        })
    }
}

/// `pages.create` 的请求：`POST /api/v1/workspaces/{ws}/pages`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesCreateRequest {
    client: Client,
    ws: String,
    body: CreatePage,
}

impl PagesCreateRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/workspaces/{}/pages", enc(&self.ws)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for PagesCreateRequest {
    type Output = Result<Page>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `pages.get` 的请求：`GET /api/v1/pages/{id}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesGetRequest {
    client: Client,
    id: String,
}

impl PagesGetRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(reqwest::Method::GET, format!("/pages/{}", enc(&self.id)));
        Ok(spec)
    }
}

impl IntoFuture for PagesGetRequest {
    type Output = Result<Page>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `pages.update` 的请求：`PATCH /api/v1/pages/{id}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesUpdateRequest {
    client: Client,
    id: String,
    body: PatchPage,
}

impl PagesUpdateRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(reqwest::Method::PATCH, format!("/pages/{}", enc(&self.id)));
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for PagesUpdateRequest {
    type Output = Result<Page>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `pages.delete` 的请求：`DELETE /api/v1/pages/{id}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesDeleteRequest {
    client: Client,
    id: String,
}

impl PagesDeleteRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(reqwest::Method::DELETE, format!("/pages/{}", enc(&self.id)));
        Ok(spec)
    }
}

impl IntoFuture for PagesDeleteRequest {
    type Output = Result<Deleted>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `pages.authors` 的请求：`GET /api/v1/pages/{id}/authors`；返回响应里的 `clients`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesAuthorsRequest {
    client: Client,
    id: String,
}

impl PagesAuthorsRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/pages/{}/authors", enc(&self.id)),
        );
        Ok(spec)
    }
}

impl IntoFuture for PagesAuthorsRequest {
    type Output = Result<std::collections::HashMap<String, AuthorRef>>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move {
            let r: Authors = client.send_json(spec?).await?;
            Ok(r.clients)
        })
    }
}

impl PagesApi {
    /// `pages.blocks.*` 的接口
    pub fn blocks(&self) -> PagesBlocksApi {
        PagesBlocksApi {
            client: self.client.clone(),
        }
    }

    /// `pages.content.*` 的接口
    pub fn content(&self) -> PagesContentApi {
        PagesContentApi {
            client: self.client.clone(),
        }
    }

    /// `pages.share.*` 的接口
    pub fn share(&self) -> PagesShareApi {
        PagesShareApi {
            client: self.client.clone(),
        }
    }

    /// `pages.versions.*` 的接口
    pub fn versions(&self) -> PagesVersionsApi {
        PagesVersionsApi {
            client: self.client.clone(),
        }
    }

    /// `GET /api/v1/workspaces/{ws}/pages`；返回响应里的 `pages`
    pub fn list(&self, ws: impl Into<String>) -> PagesListRequest {
        PagesListRequest {
            client: self.client.clone(),
            ws: ws.into(),
        }
    }

    /// `POST /api/v1/workspaces/{ws}/pages`
    pub fn create(&self, ws: impl Into<String>, body: CreatePage) -> PagesCreateRequest {
        PagesCreateRequest {
            client: self.client.clone(),
            ws: ws.into(),
            body,
        }
    }

    /// `GET /api/v1/pages/{id}`
    pub fn get(&self, id: impl Into<String>) -> PagesGetRequest {
        PagesGetRequest {
            client: self.client.clone(),
            id: id.into(),
        }
    }

    /// `PATCH /api/v1/pages/{id}`
    pub fn update(&self, id: impl Into<String>, body: PatchPage) -> PagesUpdateRequest {
        PagesUpdateRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }

    /// `DELETE /api/v1/pages/{id}`
    pub fn delete(&self, id: impl Into<String>) -> PagesDeleteRequest {
        PagesDeleteRequest {
            client: self.client.clone(),
            id: id.into(),
        }
    }

    /// `GET /api/v1/pages/{id}/authors`；返回响应里的 `clients`
    pub fn authors(&self, id: impl Into<String>) -> PagesAuthorsRequest {
        PagesAuthorsRequest {
            client: self.client.clone(),
            id: id.into(),
        }
    }
}

/// `pages.blocks.*` 的接口。
#[derive(Clone, Debug)]
pub struct PagesBlocksApi {
    pub(crate) client: Client,
}

/// `pages.blocks.insert` 的请求：`POST /api/v1/pages/{id}/blocks`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesBlocksInsertRequest {
    client: Client,
    id: String,
    body: BlockBody,
}

impl PagesBlocksInsertRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/pages/{}/blocks", enc(&self.id)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for PagesBlocksInsertRequest {
    type Output = Result<BlockIds>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `pages.blocks.get` 的请求：`GET /api/v1/pages/{id}/blocks/{block_id}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesBlocksGetRequest {
    client: Client,
    id: String,
    block_id: String,
}

impl PagesBlocksGetRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/pages/{}/blocks/{}", enc(&self.id), enc(&self.block_id)),
        );
        Ok(spec)
    }
}

impl IntoFuture for PagesBlocksGetRequest {
    type Output = Result<BlockContent>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `pages.blocks.replace` 的请求：`PUT /api/v1/pages/{id}/blocks/{block_id}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesBlocksReplaceRequest {
    client: Client,
    id: String,
    block_id: String,
    body: BlockBody,
}

impl PagesBlocksReplaceRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::PUT,
            format!("/pages/{}/blocks/{}", enc(&self.id), enc(&self.block_id)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for PagesBlocksReplaceRequest {
    type Output = Result<BlockIds>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `pages.blocks.delete` 的请求：`DELETE /api/v1/pages/{id}/blocks/{block_id}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesBlocksDeleteRequest {
    client: Client,
    id: String,
    block_id: String,
}

impl PagesBlocksDeleteRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::DELETE,
            format!("/pages/{}/blocks/{}", enc(&self.id), enc(&self.block_id)),
        );
        Ok(spec)
    }
}

impl IntoFuture for PagesBlocksDeleteRequest {
    type Output = Result<()>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_empty(spec?).await })
    }
}

impl PagesBlocksApi {
    /// `POST /api/v1/pages/{id}/blocks`
    pub fn insert(&self, id: impl Into<String>, body: BlockBody) -> PagesBlocksInsertRequest {
        PagesBlocksInsertRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }

    /// `GET /api/v1/pages/{id}/blocks/{block_id}`
    pub fn get(&self, id: impl Into<String>, block_id: impl Into<String>) -> PagesBlocksGetRequest {
        PagesBlocksGetRequest {
            client: self.client.clone(),
            id: id.into(),
            block_id: block_id.into(),
        }
    }

    /// `PUT /api/v1/pages/{id}/blocks/{block_id}`
    pub fn replace(
        &self,
        id: impl Into<String>,
        block_id: impl Into<String>,
        body: BlockBody,
    ) -> PagesBlocksReplaceRequest {
        PagesBlocksReplaceRequest {
            client: self.client.clone(),
            id: id.into(),
            block_id: block_id.into(),
            body,
        }
    }

    /// `DELETE /api/v1/pages/{id}/blocks/{block_id}`
    pub fn delete(
        &self,
        id: impl Into<String>,
        block_id: impl Into<String>,
    ) -> PagesBlocksDeleteRequest {
        PagesBlocksDeleteRequest {
            client: self.client.clone(),
            id: id.into(),
            block_id: block_id.into(),
        }
    }
}

/// `pages.content.*` 的接口。
#[derive(Clone, Debug)]
pub struct PagesContentApi {
    pub(crate) client: Client,
}

/// `pages.content.get` 的请求：`GET /api/v1/pages/{id}/content`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesContentGetRequest {
    client: Client,
    id: String,
    format: Option<String>,
    limit: Option<u64>,
}

impl PagesContentGetRequest {
    /// markdown（默认）/ json / html（只读 HTML，见 `Content.html`）（查询参数 `format`）
    pub fn format(mut self, v: impl Into<String>) -> Self {
        self.format = Some(v.into());
        self
    }

    /// 只要前 N 个顶层块（先画第一屏），响应里带 `total`（查询参数 `limit`）
    pub fn limit(mut self, v: u64) -> Self {
        self.limit = Some(v);
        self
    }

    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/pages/{}/content", enc(&self.id)),
        );
        if let Some(v) = &self.format {
            spec.query("format", v.to_string());
        }
        if let Some(v) = &self.limit {
            spec.query("limit", v.to_string());
        }
        Ok(spec)
    }
}

impl IntoFuture for PagesContentGetRequest {
    type Output = Result<Content>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `pages.content.replace` 的请求：`PUT /api/v1/pages/{id}/content`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesContentReplaceRequest {
    client: Client,
    id: String,
    body: PutContent,
}

impl PagesContentReplaceRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::PUT,
            format!("/pages/{}/content", enc(&self.id)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for PagesContentReplaceRequest {
    type Output = Result<()>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_empty(spec?).await })
    }
}

impl PagesContentApi {
    /// `GET /api/v1/pages/{id}/content`
    pub fn get(&self, id: impl Into<String>) -> PagesContentGetRequest {
        PagesContentGetRequest {
            client: self.client.clone(),
            id: id.into(),
            format: None,
            limit: None,
        }
    }

    /// `PUT /api/v1/pages/{id}/content`
    pub fn replace(&self, id: impl Into<String>, body: PutContent) -> PagesContentReplaceRequest {
        PagesContentReplaceRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }
}

/// `pages.share.*` 的接口。
#[derive(Clone, Debug)]
pub struct PagesShareApi {
    pub(crate) client: Client,
}

/// `pages.share.get` 的请求：`GET /api/v1/pages/{id}/share`；404 时返回 `None`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesShareGetRequest {
    client: Client,
    id: String,
}

impl PagesShareGetRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/pages/{}/share", enc(&self.id)),
        );
        Ok(spec)
    }
}

impl IntoFuture for PagesShareGetRequest {
    type Output = Result<Option<ShareView>>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_optional(spec?).await })
    }
}

/// `pages.share.enable` 的请求：打开分享或改设置（已经分享了的，链接不变）。
///
/// `POST /api/v1/pages/{id}/share`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesShareEnableRequest {
    client: Client,
    id: String,
    body: PutShare,
}

impl PagesShareEnableRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/pages/{}/share", enc(&self.id)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for PagesShareEnableRequest {
    type Output = Result<ShareView>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `pages.share.disable` 的请求：关掉分享，链接马上失效。
///
/// `DELETE /api/v1/pages/{id}/share`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesShareDisableRequest {
    client: Client,
    id: String,
}

impl PagesShareDisableRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::DELETE,
            format!("/pages/{}/share", enc(&self.id)),
        );
        Ok(spec)
    }
}

impl IntoFuture for PagesShareDisableRequest {
    type Output = Result<()>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_empty(spec?).await })
    }
}

impl PagesShareApi {
    /// `GET /api/v1/pages/{id}/share`；404 时返回 `None`
    pub fn get(&self, id: impl Into<String>) -> PagesShareGetRequest {
        PagesShareGetRequest {
            client: self.client.clone(),
            id: id.into(),
        }
    }

    /// 打开分享或改设置（已经分享了的，链接不变）。
    ///
    /// `POST /api/v1/pages/{id}/share`
    pub fn enable(&self, id: impl Into<String>, body: PutShare) -> PagesShareEnableRequest {
        PagesShareEnableRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }

    /// 关掉分享，链接马上失效。
    ///
    /// `DELETE /api/v1/pages/{id}/share`
    pub fn disable(&self, id: impl Into<String>) -> PagesShareDisableRequest {
        PagesShareDisableRequest {
            client: self.client.clone(),
            id: id.into(),
        }
    }
}

/// `pages.versions.*` 的接口。
#[derive(Clone, Debug)]
pub struct PagesVersionsApi {
    pub(crate) client: Client,
}

/// `pages.versions.list` 的请求：`GET /api/v1/pages/{id}/versions`。一页；逐条取完所有页用 `list_all` 或 `.into_stream()`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesVersionsListRequest {
    client: Client,
    id: String,
    limit: Option<u64>,
    cursor: Option<String>,
}

impl crate::Paginated for PagesVersionsListRequest {
    type Item = VersionMeta;

    fn cursor(&self) -> Option<String> {
        self.cursor.clone()
    }

    fn with_cursor(&self, cursor: Option<String>) -> Self {
        let mut r = self.clone();
        r.cursor = cursor;
        r
    }

    fn fetch(self) -> crate::pagination::PageFuture<VersionMeta> {
        Box::pin(async move {
            let p: VersionList = self.await?;
            Ok((p.versions, p.next_cursor))
        })
    }
}

impl crate::ListAll<PagesVersionsListRequest> {
    /// 一页几条，默认 200
    pub fn limit(self, v: u64) -> Self {
        self.map_request(|r| r.limit(v))
    }

    /// 上一页给的 `next_cursor`
    pub fn cursor(self, v: impl Into<String>) -> Self {
        self.map_request(|r| r.cursor(v.into()))
    }
}

impl PagesVersionsListRequest {
    /// 一页几条，默认 200（查询参数 `limit`）
    pub fn limit(mut self, v: u64) -> Self {
        self.limit = Some(v);
        self
    }

    /// 上一页给的 `next_cursor`（查询参数 `cursor`）
    pub fn cursor(mut self, v: impl Into<String>) -> Self {
        self.cursor = Some(v.into());
        self
    }

    /// 逐条取完所有页（自动带 `cursor`）。
    pub fn into_stream(self) -> crate::ListAll<Self> {
        crate::ListAll::new(self)
    }

    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/pages/{}/versions", enc(&self.id)),
        );
        if let Some(v) = &self.limit {
            spec.query("limit", v.to_string());
        }
        if let Some(v) = &self.cursor {
            spec.query("cursor", v.to_string());
        }
        Ok(spec)
    }
}

impl IntoFuture for PagesVersionsListRequest {
    type Output = Result<VersionList>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `pages.versions.create` 的请求：`POST /api/v1/pages/{id}/versions`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesVersionsCreateRequest {
    client: Client,
    id: String,
    body: SaveBody,
}

impl PagesVersionsCreateRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/pages/{}/versions", enc(&self.id)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for PagesVersionsCreateRequest {
    type Output = Result<VersionMeta>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `pages.versions.get` 的请求：`GET /api/v1/pages/{id}/versions/{v}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesVersionsGetRequest {
    client: Client,
    id: String,
    v: i64,
}

impl PagesVersionsGetRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/pages/{}/versions/{}", enc(&self.id), self.v),
        );
        Ok(spec)
    }
}

impl IntoFuture for PagesVersionsGetRequest {
    type Output = Result<VersionContent>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `pages.versions.diff` 的请求：`GET /api/v1/pages/{id}/versions/{v}/diff`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesVersionsDiffRequest {
    client: Client,
    id: String,
    v: i64,
    to: Option<String>,
}

impl PagesVersionsDiffRequest {
    /// 和哪一版比：版本号，或 `current`（默认，现在的正文）（查询参数 `to`）
    pub fn to(mut self, v: impl Into<String>) -> Self {
        self.to = Some(v.into());
        self
    }

    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/pages/{}/versions/{}/diff", enc(&self.id), self.v),
        );
        if let Some(v) = &self.to {
            spec.query("to", v.to_string());
        }
        Ok(spec)
    }
}

impl IntoFuture for PagesVersionsDiffRequest {
    type Output = Result<VersionDiff>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `pages.versions.restore` 的请求：`POST /api/v1/pages/{id}/versions/{v}/restore`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct PagesVersionsRestoreRequest {
    client: Client,
    id: String,
    v: i64,
}

impl PagesVersionsRestoreRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/pages/{}/versions/{}/restore", enc(&self.id), self.v),
        );
        Ok(spec)
    }
}

impl IntoFuture for PagesVersionsRestoreRequest {
    type Output = Result<Restored>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

impl PagesVersionsApi {
    /// `GET /api/v1/pages/{id}/versions`。一页；逐条取完所有页用 `list_all` 或 `.into_stream()`
    pub fn list(&self, id: impl Into<String>) -> PagesVersionsListRequest {
        PagesVersionsListRequest {
            client: self.client.clone(),
            id: id.into(),
            limit: None,
            cursor: None,
        }
    }

    /// 和 [`list`](Self::list) 一样，但逐条取完所有页：返回 `Stream<Item = Result<条目>>`，停止条件是没有下一页、这页是空的或游标没变。
    pub fn list_all(&self, id: impl Into<String>) -> crate::ListAll<PagesVersionsListRequest> {
        self.list(id).into_stream()
    }

    /// `POST /api/v1/pages/{id}/versions`
    pub fn create(&self, id: impl Into<String>, body: SaveBody) -> PagesVersionsCreateRequest {
        PagesVersionsCreateRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }

    /// `GET /api/v1/pages/{id}/versions/{v}`
    pub fn get(&self, id: impl Into<String>, v: i64) -> PagesVersionsGetRequest {
        PagesVersionsGetRequest {
            client: self.client.clone(),
            id: id.into(),
            v,
        }
    }

    /// `GET /api/v1/pages/{id}/versions/{v}/diff`
    pub fn diff(&self, id: impl Into<String>, v: i64) -> PagesVersionsDiffRequest {
        PagesVersionsDiffRequest {
            client: self.client.clone(),
            id: id.into(),
            v,
            to: None,
        }
    }

    /// `POST /api/v1/pages/{id}/versions/{v}/restore`
    pub fn restore(&self, id: impl Into<String>, v: i64) -> PagesVersionsRestoreRequest {
        PagesVersionsRestoreRequest {
            client: self.client.clone(),
            id: id.into(),
            v,
        }
    }
}

/// `threads.*` 的接口。
#[derive(Clone, Debug)]
pub struct ThreadsApi {
    pub(crate) client: Client,
}

/// `threads.list` 的请求：`GET /api/v1/pages/{id}/comments`；返回响应里的 `threads`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct ThreadsListRequest {
    client: Client,
    id: String,
    resolved: Option<String>,
}

impl ThreadsListRequest {
    /// false（默认，没解决的）/ true / all（查询参数 `resolved`）
    pub fn resolved(mut self, v: impl Into<String>) -> Self {
        self.resolved = Some(v.into());
        self
    }

    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/pages/{}/comments", enc(&self.id)),
        );
        if let Some(v) = &self.resolved {
            spec.query("resolved", v.to_string());
        }
        Ok(spec)
    }
}

impl IntoFuture for ThreadsListRequest {
    type Output = Result<Vec<ThreadView>>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move {
            let r: ThreadList = client.send_json(spec?).await?;
            Ok(r.threads)
        })
    }
}

/// `threads.create` 的请求：`POST /api/v1/pages/{id}/comments`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct ThreadsCreateRequest {
    client: Client,
    id: String,
    body: NewThread,
}

impl ThreadsCreateRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/pages/{}/comments", enc(&self.id)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for ThreadsCreateRequest {
    type Output = Result<ThreadView>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `threads.update` 的请求：`PATCH /api/v1/threads/{id}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct ThreadsUpdateRequest {
    client: Client,
    id: String,
    body: PatchThread,
}

impl ThreadsUpdateRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::PATCH,
            format!("/threads/{}", enc(&self.id)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for ThreadsUpdateRequest {
    type Output = Result<ThreadView>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `threads.reply` 的请求：`POST /api/v1/threads/{id}/comments`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct ThreadsReplyRequest {
    client: Client,
    id: String,
    body: Reply,
}

impl ThreadsReplyRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/threads/{}/comments", enc(&self.id)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for ThreadsReplyRequest {
    type Output = Result<CommentView>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

impl ThreadsApi {
    /// `GET /api/v1/pages/{id}/comments`；返回响应里的 `threads`
    pub fn list(&self, id: impl Into<String>) -> ThreadsListRequest {
        ThreadsListRequest {
            client: self.client.clone(),
            id: id.into(),
            resolved: None,
        }
    }

    /// `POST /api/v1/pages/{id}/comments`
    pub fn create(&self, id: impl Into<String>, body: NewThread) -> ThreadsCreateRequest {
        ThreadsCreateRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }

    /// `PATCH /api/v1/threads/{id}`
    pub fn update(&self, id: impl Into<String>, body: PatchThread) -> ThreadsUpdateRequest {
        ThreadsUpdateRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }

    /// `POST /api/v1/threads/{id}/comments`
    pub fn reply(&self, id: impl Into<String>, body: Reply) -> ThreadsReplyRequest {
        ThreadsReplyRequest {
            client: self.client.clone(),
            id: id.into(),
            body,
        }
    }
}

/// `trash.*` 的接口。
#[derive(Clone, Debug)]
pub struct TrashApi {
    pub(crate) client: Client,
}

/// `trash.list` 的请求：`GET /api/v1/workspaces/{ws}/trash`。一页；逐条取完所有页用 `list_all` 或 `.into_stream()`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct TrashListRequest {
    client: Client,
    ws: String,
    limit: Option<u64>,
    cursor: Option<String>,
}

impl crate::Paginated for TrashListRequest {
    type Item = TrashItem;

    fn cursor(&self) -> Option<String> {
        self.cursor.clone()
    }

    fn with_cursor(&self, cursor: Option<String>) -> Self {
        let mut r = self.clone();
        r.cursor = cursor;
        r
    }

    fn fetch(self) -> crate::pagination::PageFuture<TrashItem> {
        Box::pin(async move {
            let p: TrashList = self.await?;
            Ok((p.items, p.next_cursor))
        })
    }
}

impl crate::ListAll<TrashListRequest> {
    /// 一页几条，默认 200
    pub fn limit(self, v: u64) -> Self {
        self.map_request(|r| r.limit(v))
    }

    /// 上一页给的 `next_cursor`
    pub fn cursor(self, v: impl Into<String>) -> Self {
        self.map_request(|r| r.cursor(v.into()))
    }
}

impl TrashListRequest {
    /// 一页几条，默认 200（查询参数 `limit`）
    pub fn limit(mut self, v: u64) -> Self {
        self.limit = Some(v);
        self
    }

    /// 上一页给的 `next_cursor`（查询参数 `cursor`）
    pub fn cursor(mut self, v: impl Into<String>) -> Self {
        self.cursor = Some(v.into());
        self
    }

    /// 逐条取完所有页（自动带 `cursor`）。
    pub fn into_stream(self) -> crate::ListAll<Self> {
        crate::ListAll::new(self)
    }

    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/workspaces/{}/trash", enc(&self.ws)),
        );
        if let Some(v) = &self.limit {
            spec.query("limit", v.to_string());
        }
        if let Some(v) = &self.cursor {
            spec.query("cursor", v.to_string());
        }
        Ok(spec)
    }
}

impl IntoFuture for TrashListRequest {
    type Output = Result<TrashList>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `trash.restore` 的请求：`POST /api/v1/trash/{id}/restore`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct TrashRestoreRequest {
    client: Client,
    id: String,
}

impl TrashRestoreRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::POST,
            format!("/trash/{}/restore", enc(&self.id)),
        );
        Ok(spec)
    }
}

impl IntoFuture for TrashRestoreRequest {
    type Output = Result<RestoredPage>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `trash.purge` 的请求：`DELETE /api/v1/trash/{id}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct TrashPurgeRequest {
    client: Client,
    id: String,
}

impl TrashPurgeRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(reqwest::Method::DELETE, format!("/trash/{}", enc(&self.id)));
        Ok(spec)
    }
}

impl IntoFuture for TrashPurgeRequest {
    type Output = Result<()>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_empty(spec?).await })
    }
}

impl TrashApi {
    /// `GET /api/v1/workspaces/{ws}/trash`。一页；逐条取完所有页用 `list_all` 或 `.into_stream()`
    pub fn list(&self, ws: impl Into<String>) -> TrashListRequest {
        TrashListRequest {
            client: self.client.clone(),
            ws: ws.into(),
            limit: None,
            cursor: None,
        }
    }

    /// 和 [`list`](Self::list) 一样，但逐条取完所有页：返回 `Stream<Item = Result<条目>>`，停止条件是没有下一页、这页是空的或游标没变。
    pub fn list_all(&self, ws: impl Into<String>) -> crate::ListAll<TrashListRequest> {
        self.list(ws).into_stream()
    }

    /// `POST /api/v1/trash/{id}/restore`
    pub fn restore(&self, id: impl Into<String>) -> TrashRestoreRequest {
        TrashRestoreRequest {
            client: self.client.clone(),
            id: id.into(),
        }
    }

    /// `DELETE /api/v1/trash/{id}`
    pub fn purge(&self, id: impl Into<String>) -> TrashPurgeRequest {
        TrashPurgeRequest {
            client: self.client.clone(),
            id: id.into(),
        }
    }
}

/// `workspaces.*` 的接口。
#[derive(Clone, Debug)]
pub struct WorkspacesApi {
    pub(crate) client: Client,
}

/// `workspaces.update` 的请求：改工作区的名字、图标（admin 以上）。
///
/// `PATCH /api/v1/workspaces/{ws}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct WorkspacesUpdateRequest {
    client: Client,
    ws: String,
    body: WsPatch,
}

impl WorkspacesUpdateRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::PATCH,
            format!("/workspaces/{}", enc(&self.ws)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for WorkspacesUpdateRequest {
    type Output = Result<Workspace>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `workspaces.events` 的请求：SSE：工作区的实时事件，事件名和数据见 webhooks（page.created、db.changed……）。慢了会收到 resync。
///
/// `GET /api/v1/workspaces/{ws}/events`；`.await` 连上以后得到事件流，断了自动重连（带 `Last-Event-ID`）
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct WorkspacesEventsRequest {
    client: Client,
    ws: String,
}

impl WorkspacesEventsRequest {
    fn spec(&self, last_event_id: Option<&str>) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/workspaces/{}/events", enc(&self.ws)),
        );
        if let Some(id) = last_event_id {
            spec.header("last-event-id", id);
        }
        Ok(spec)
    }
}

impl IntoFuture for WorkspacesEventsRequest {
    type Output = Result<crate::events::EventStream<WorkspaceEvent>>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let client = self.client.clone();
            client
                .open_events::<WorkspaceEvent>(std::sync::Arc::new(move |last: Option<&str>| {
                    self.spec(last)
                }))
                .await
        })
    }
}

impl WorkspacesApi {
    /// `workspaces.members.*` 的接口
    pub fn members(&self) -> WorkspacesMembersApi {
        WorkspacesMembersApi {
            client: self.client.clone(),
        }
    }

    /// 改工作区的名字、图标（admin 以上）。
    ///
    /// `PATCH /api/v1/workspaces/{ws}`
    pub fn update(&self, ws: impl Into<String>, body: WsPatch) -> WorkspacesUpdateRequest {
        WorkspacesUpdateRequest {
            client: self.client.clone(),
            ws: ws.into(),
            body,
        }
    }

    /// SSE：工作区的实时事件，事件名和数据见 webhooks（page.created、db.changed……）。慢了会收到 resync。
    ///
    /// `GET /api/v1/workspaces/{ws}/events`；`.await` 连上以后得到事件流，断了自动重连（带 `Last-Event-ID`）
    pub fn events(&self, ws: impl Into<String>) -> WorkspacesEventsRequest {
        WorkspacesEventsRequest {
            client: self.client.clone(),
            ws: ws.into(),
        }
    }
}

/// `workspaces.members.*` 的接口。
#[derive(Clone, Debug)]
pub struct WorkspacesMembersApi {
    pub(crate) client: Client,
}

/// `workspaces.members.list` 的请求：`GET /api/v1/workspaces/{ws}/members`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct WorkspacesMembersListRequest {
    client: Client,
    ws: String,
}

impl WorkspacesMembersListRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::GET,
            format!("/workspaces/{}/members", enc(&self.ws)),
        );
        Ok(spec)
    }
}

impl IntoFuture for WorkspacesMembersListRequest {
    type Output = Result<MemberList>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `workspaces.members.set_role` 的请求：`PATCH /api/v1/workspaces/{ws}/members/{user}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct WorkspacesMembersSetRoleRequest {
    client: Client,
    ws: String,
    user: String,
    body: RoleBody,
}

impl WorkspacesMembersSetRoleRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::PATCH,
            format!("/workspaces/{}/members/{}", enc(&self.ws), enc(&self.user)),
        );
        spec.json(&self.body)?;
        Ok(spec)
    }
}

impl IntoFuture for WorkspacesMembersSetRoleRequest {
    type Output = Result<MemberRole>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_json(spec?).await })
    }
}

/// `workspaces.members.remove` 的请求：`DELETE /api/v1/workspaces/{ws}/members/{user}`
#[derive(Clone, Debug)]
#[must_use = "请求要 .await 才会发出去"]
pub struct WorkspacesMembersRemoveRequest {
    client: Client,
    ws: String,
    user: String,
}

impl WorkspacesMembersRemoveRequest {
    fn spec(&self) -> Result<Spec> {
        #[allow(unused_mut)]
        let mut spec = Spec::new(
            reqwest::Method::DELETE,
            format!("/workspaces/{}/members/{}", enc(&self.ws), enc(&self.user)),
        );
        Ok(spec)
    }
}

impl IntoFuture for WorkspacesMembersRemoveRequest {
    type Output = Result<()>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        let spec = self.spec();
        let client = self.client;
        Box::pin(async move { client.send_empty(spec?).await })
    }
}

impl WorkspacesMembersApi {
    /// `GET /api/v1/workspaces/{ws}/members`
    pub fn list(&self, ws: impl Into<String>) -> WorkspacesMembersListRequest {
        WorkspacesMembersListRequest {
            client: self.client.clone(),
            ws: ws.into(),
        }
    }

    /// `PATCH /api/v1/workspaces/{ws}/members/{user}`
    pub fn set_role(
        &self,
        ws: impl Into<String>,
        user: impl Into<String>,
        body: RoleBody,
    ) -> WorkspacesMembersSetRoleRequest {
        WorkspacesMembersSetRoleRequest {
            client: self.client.clone(),
            ws: ws.into(),
            user: user.into(),
            body,
        }
    }

    /// `DELETE /api/v1/workspaces/{ws}/members/{user}`
    pub fn remove(
        &self,
        ws: impl Into<String>,
        user: impl Into<String>,
    ) -> WorkspacesMembersRemoveRequest {
        WorkspacesMembersRemoveRequest {
            client: self.client.clone(),
            ws: ws.into(),
            user: user.into(),
        }
    }
}
