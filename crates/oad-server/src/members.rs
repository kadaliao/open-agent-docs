//! 成员和邀请、个人资料、注册。见 docs/phase5-contract.md。
use crate::api::{ApiResult, Auth, Body, double, me_json, session_cookie};
use crate::{ApiError, AppState};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use oad_core::{AuthRequest, Invite, PrefsPatch, Role};
use serde::Deserialize;
use serde_json::{Value, json};

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

fn invite_json(s: &AppState, i: &Invite) -> Value {
    json!({
        "token": i.token,
        "url": s.core.invite_url(&i.token),
        "role": i.role,
        "expires_at": i.expires_at,
        "created_at": i.created_at,
        "uses": i.uses,
    })
}

fn parse_role(s: &str) -> ApiResult<Role> {
    Role::parse(s).ok_or_else(|| ApiError::bad_request("role 只能是 viewer、editor、admin、owner"))
}

#[derive(Deserialize)]
struct NewInvite {
    role: String,
    expires_in: Option<i64>,
}

async fn create_invite(
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

async fn list_invites(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
) -> ApiResult<Json<Value>> {
    let list: Vec<Value> = s
        .core
        .list_invites(&ws, &p)
        .await?
        .iter()
        .map(|i| invite_json(&s, i))
        .collect();
    Ok(Json(json!({ "invites": list })))
}

async fn revoke_invite(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((ws, token)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    s.core.revoke_invite(&ws, &token, &p).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn invite_info(
    State(s): State<AppState>,
    Path(token): Path<String>,
) -> ApiResult<Json<Value>> {
    Ok(Json(
        serde_json::to_value(s.core.invite_info(&token).await?).unwrap_or_default(),
    ))
}

async fn accept(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(token): Path<String>,
) -> ApiResult<Json<Value>> {
    s.core.accept_invite(&token, &p).await?;
    Ok(Json(me_json(&s, &p).await?))
}

#[derive(Deserialize)]
struct RoleBody {
    role: String,
}

async fn set_role(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((ws, user)): Path<(String, String)>,
    Body(b): Body<RoleBody>,
) -> ApiResult<Json<Value>> {
    let role = parse_role(&b.role)?;
    s.core.set_member_role(&ws, &user, role, &p).await?;
    Ok(Json(json!({ "user_id": user, "role": role })))
}

async fn remove_member(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path((ws, user)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    s.core.remove_member(&ws, &user, &p).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct WsPatch {
    name: Option<String>,
    #[serde(default, deserialize_with = "double")]
    icon: Option<Option<String>>,
}

async fn update_ws(
    State(s): State<AppState>,
    Auth(p): Auth,
    Path(ws): Path<String>,
    Body(b): Body<WsPatch>,
) -> ApiResult<Json<Value>> {
    let w = s.core.update_workspace(&ws, &p, b.name, b.icon).await?;
    Ok(Json(serde_json::to_value(w).unwrap_or_default()))
}

#[derive(Deserialize)]
pub(crate) struct MePatch {
    name: Option<String>,
    #[serde(default, deserialize_with = "double")]
    avatar: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    locale: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    timezone: Option<Option<String>>,
}

pub(crate) async fn update_me(
    State(s): State<AppState>,
    Auth(p): Auth,
    Body(b): Body<MePatch>,
) -> ApiResult<Json<Value>> {
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

#[derive(Deserialize)]
struct PasswordBody {
    current: String,
    new: String,
}

async fn change_password(
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

#[derive(Deserialize)]
struct SignupBody {
    email: String,
    password: String,
    #[serde(default)]
    name: String,
    invite: Option<String>,
}

async fn signup(State(s): State<AppState>, Body(b): Body<SignupBody>) -> ApiResult<Response> {
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
