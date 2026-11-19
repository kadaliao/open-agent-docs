//! 成员和邀请、个人资料、注册。见 docs/phase5-contract.md。
use crate::api::{ApiResult, Auth, Body, Me, double, me_json, session_cookie};
use crate::openapi::ErrorBody;
use crate::{ApiError, AppState};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use oad_core::members::InviteInfo;
use oad_core::{AuthRequest, Invite, PrefsPatch, Role, Workspace};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/workspaces/{ws}", patch(update_ws))
        .route(
            "/workspaces/{ws}/invites",
            get(list_invites).post(create_invite),
        )
        .route("/workspaces/{ws}/invites/{token}", delete(revoke_invite))
        .route(
            "/workspaces/{ws}/members/{user}",
            patch(set_role).delete(remove_member),
        )
        .route("/invites/{token}", get(invite_info))
        .route("/invites/{token}/accept", post(accept))
        .route("/me/password", post(change_password))
        .route("/auth/signup", post(signup))
}

/// 一条邀请链接。
#[derive(Serialize, ToSchema)]
pub(crate) struct InviteView {
    token: String,
    /// 发给别人的完整链接
    url: String,
    /// 凭它加入后的角色
    role: Role,
    /// 毫秒时间戳；null = 不过期
    expires_at: Option<i64>,
    created_at: i64,
    /// 用过几次
    uses: i64,
}

fn invite_json(s: &AppState, i: &Invite) -> InviteView {
    InviteView {
        token: i.token.clone(),
        url: s.core.invite_url(&i.token),
        role: i.role,
        expires_at: i.expires_at,
        created_at: i.created_at,
        uses: i.uses,
    }
}

fn parse_role(s: &str) -> ApiResult<Role> {
    Role::parse(s).ok_or_else(|| ApiError::bad_request("role 只能是 viewer、editor、admin、owner"))
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct NewInvite {
    /// viewer / editor / admin / owner
    role: String,
    /// 多少秒后过期；不给 = 不过期
    expires_in: Option<i64>,
}

#[utoipa::path(post, path = "/workspaces/{ws}/invites", tag = "invites", params(("ws" = String, Path, description = "工作区")), request_body = NewInvite,
    responses((status = 201, body = InviteView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn create_invite(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
    Body(b): Body<NewInvite>,
) -> ApiResult<Response> {
    let i = s
        .core
        .create_invite(&ws, &p, parse_role(&b.role)?, b.expires_in)
        .await?;
    Ok((StatusCode::CREATED, Json(invite_json(&s, &i))).into_response())
}

#[derive(Serialize, ToSchema)]
pub(crate) struct InviteList {
    /// 还能用的
    invites: Vec<InviteView>,
}

#[utoipa::path(get, path = "/workspaces/{ws}/invites", tag = "invites", params(("ws" = String, Path, description = "工作区")),
    responses((status = 200, body = InviteList), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn list_invites(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
) -> ApiResult<Json<InviteList>> {
    let invites = s
        .core
        .list_invites(&ws, &p)
        .await?
        .iter()
        .map(|i| invite_json(&s, i))
        .collect();
    Ok(Json(InviteList { invites }))
}

#[utoipa::path(delete, path = "/workspaces/{ws}/invites/{token}", tag = "invites",
    params(("ws" = String, Path, description = "工作区"), ("token" = String, Path, description = "邀请")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn revoke_invite(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((ws, token)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    s.core.revoke_invite(&ws, &token, &p).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/invites/{token}", tag = "invites", security(()), params(("token" = String, Path, description = "邀请")),
    description = "打开邀请链接时看的：哪个工作区、什么角色、谁邀请的。不用登录。",
    responses((status = 200, body = InviteInfo), (status = 404, body = ErrorBody), (status = 410, body = ErrorBody, description = "过期或撤销了")))]
pub(crate) async fn invite_info(
    State(s): State<AppState>,
    Path(token): Path<String>,
) -> ApiResult<Json<InviteInfo>> {
    Ok(Json(s.core.invite_info(&token).await?))
}

#[utoipa::path(post, path = "/invites/{token}/accept", tag = "invites", params(("token" = String, Path, description = "邀请")),
    description = "接受邀请，加入工作区（已经在里面的不降角色）。返回加入后的 /me。",
    responses((status = 200, body = Me), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
        (status = 410, body = ErrorBody, description = "过期或撤销了")))]
pub(crate) async fn accept(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(token): Path<String>,
) -> ApiResult<Json<Me>> {
    s.core.accept_invite(&token, &p).await?;
    Ok(Json(me_json(&s, &p).await?))
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct RoleBody {
    /// viewer / editor / admin / owner
    role: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct MemberRole {
    user_id: String,
    role: Role,
}

#[utoipa::path(patch, path = "/workspaces/{ws}/members/{user}", tag = "members",
    params(("ws" = String, Path, description = "工作区"), ("user" = String, Path, description = "用户")), request_body = RoleBody,
    responses((status = 200, body = MemberRole), (status = 400, body = ErrorBody, description = "角色不对，或者会去掉最后一个所有者"),
        (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn set_role(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((ws, user)): Path<(String, String)>,
    Body(b): Body<RoleBody>,
) -> ApiResult<Json<MemberRole>> {
    let role = parse_role(&b.role)?;
    s.core.set_member_role(&ws, &user, role, &p).await?;
    Ok(Json(MemberRole {
        user_id: user,
        role,
    }))
}

#[utoipa::path(delete, path = "/workspaces/{ws}/members/{user}", tag = "members",
    params(("ws" = String, Path, description = "工作区"), ("user" = String, Path, description = "用户")),
    responses((status = 204), (status = 400, body = ErrorBody, description = "会去掉最后一个所有者"), (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn remove_member(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((ws, user)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    s.core.remove_member(&ws, &user, &p).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct WsPatch {
    /// 新名字
    name: Option<String>,
    /// 图标（emoji 或图片地址）；null = 去掉
    #[serde(default, deserialize_with = "double")]
    icon: Option<Option<String>>,
}

#[utoipa::path(patch, path = "/workspaces/{ws}", tag = "members", params(("ws" = String, Path, description = "工作区")), request_body = WsPatch,
    description = "改工作区的名字、图标（admin 以上）。",
    responses((status = 200, body = Workspace), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub(crate) async fn update_ws(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
    Body(b): Body<WsPatch>,
) -> ApiResult<Json<Workspace>> {
    let w = s.core.update_workspace(&ws, &p, b.name, b.icon).await?;
    Ok(Json(w))
}

/// 改个人资料：只改给了的字段；给 null 是清掉。
#[derive(Deserialize, ToSchema)]
pub(crate) struct MePatch {
    name: Option<String>,
    /// 头像（emoji 或图片地址）
    #[serde(default, deserialize_with = "double")]
    avatar: Option<Option<String>>,
    /// zh / en；null = 跟浏览器
    #[serde(default, deserialize_with = "double")]
    locale: Option<Option<String>>,
    /// IANA 时区名；null = 跟服务器
    #[serde(default, deserialize_with = "double")]
    timezone: Option<Option<String>>,
}

#[utoipa::path(patch, path = "/me", tag = "me", request_body = MePatch,
    responses((status = 200, body = Me), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)))]
pub(crate) async fn update_me(
    State(s): State<AppState>,
    Auth(p): Auth,
    Body(b): Body<MePatch>,
) -> ApiResult<Json<Me>> {
    s.core
        .update_me(
            &p,
            PrefsPatch {
                name: b.name,
                avatar: b.avatar,
                locale: b.locale,
                timezone: b.timezone,
            },
        )
        .await?;
    Ok(Json(me_json(&s, &p).await?))
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct PasswordBody {
    /// 现在的密码
    current: String,
    /// 新密码
    new: String,
}

#[utoipa::path(post, path = "/me/password", tag = "me", request_body = PasswordBody,
    description = "改密码（本地登录）。别的会话都退出，这次请求用的会话保留。",
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
pub(crate) async fn change_password(
    State(s): State<AppState>,
    Auth(p): Auth,
    headers: HeaderMap,
    Body(b): Body<PasswordBody>,
) -> ApiResult<StatusCode> {
    let req = AuthRequest {
        headers: &headers,
        query_token: None,
    };
    let keep = req
        .session_tokens()
        .first()
        .map(|t| oad_core::ids::hash_token(t));
    s.core
        .change_password(&p, &b.current, &b.new, keep.as_deref())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct SignupBody {
    email: String,
    password: String,
    /// 显示的名字；不给就用邮箱 @ 前面那段
    #[serde(default)]
    name: String,
    /// 邀请的 token（OAD_SIGNUP=invite 时必须有），注册完直接加入那个工作区
    invite: Option<String>,
}

#[utoipa::path(post, path = "/auth/signup", tag = "auth", security(()), request_body = SignupBody,
    description = "注册（本地登录模式）。OAD_SIGNUP：invite（默认，要凭邀请）、open、off。",
    responses((status = 201, body = Me, description = "注册了，Set-Cookie 带会话"), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody), (status = 409, body = ErrorBody, description = "邮箱注册过了"), (status = 410, body = ErrorBody, description = "邀请过期了"),
        (status = 429, body = ErrorBody)))]
pub(crate) async fn signup(
    State(s): State<AppState>,
    Body(b): Body<SignupBody>,
) -> ApiResult<Response> {
    let (token, user) = s
        .core
        .signup(&b.email, &b.password, &b.name, b.invite.as_deref())
        .await?;
    let me = me_json(&s, &oad_core::Principal::from(&user)).await?;
    let max_age = oad_core::auth::SESSION_DAYS * 24 * 3600;
    let mut res = (StatusCode::CREATED, Json(me)).into_response();
    res.headers_mut()
        .insert(header::SET_COOKIE, session_cookie(&s, &token, max_age));
    Ok(res)
}

/// 这个模块的接口（合进 [`crate::openapi`]）。
#[derive(OpenApi)]
#[openapi(paths(
    update_ws,
    list_invites,
    create_invite,
    revoke_invite,
    set_role,
    remove_member,
    invite_info,
    accept,
    update_me,
    change_password,
    signup
))]
pub(crate) struct Doc;
