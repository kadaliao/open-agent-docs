//! 成员和邀请：邀请链接（角色、过期）、改角色、移出、工作区改名；个人资料（名字、头像、语言）、改密码；
//! 本地登录模式下的注册（`OAD_SIGNUP`：invite 默认，只能凭邀请；open 谁都能注册；off 不能注册）。
use crate::auth::{AuthMode, SESSION_DAYS, hash_password, verify_password};
use crate::model::*;
use crate::{Core, Error, Result, ids};
use serde::Serialize;

/// 注册方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum SignupMode {
    Invite = 0,
    Open = 1,
    Off = 2,
}

impl SignupMode {
    /// `OAD_SIGNUP`，默认 invite。
    pub fn from_env() -> SignupMode {
        match std::env::var("OAD_SIGNUP").unwrap_or_default().trim() {
            "open" => SignupMode::Open,
            "off" => SignupMode::Off,
            _ => SignupMode::Invite,
        }
    }
}

/// 邀请链接打开时看到的（不用登录）。
#[derive(Serialize, Clone, Debug)]
pub struct InviteInfo {
    pub workspace: serde_json::Value,
    pub role: Role,
    pub invited_by: Option<serde_json::Value>,
    pub expires_at: Option<i64>,
    /// 能不能凭它注册
    pub signup: bool,
}

fn clean_name(s: &str) -> Result<String> {
    let n: String = s
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(80)
        .collect();
    if n.is_empty() {
        return Err(Error::BadRequest("名字不能是空的".into()));
    }
    Ok(n)
}

impl Core {
    pub fn signup_mode(&self) -> SignupMode {
        match self.signup.load(std::sync::atomic::Ordering::Relaxed) {
            1 => SignupMode::Open,
            2 => SignupMode::Off,
            _ => SignupMode::Invite,
        }
    }

    /// 改注册方式（启动时按 `OAD_SIGNUP`；测试、宿主程序可以改）。
    pub fn set_signup_mode(&self, m: SignupMode) {
        self.signup
            .store(m as u8, std::sync::atomic::Ordering::Relaxed);
    }

    /// 能管成员的人（admin / owner；Agent 不能）。
    async fn require_manager(&self, workspace_id: &str, who: &Principal) -> Result<Role> {
        if who.kind == PrincipalKind::Agent {
            return Err(Error::Forbidden("Agent 不能管成员".into()));
        }
        let role = self.require(workspace_id, who, false).await?;
        if !role.can_manage() {
            return Err(Error::Forbidden("只有管理员能管成员和邀请".into()));
        }
        Ok(role)
    }

    /// 邀请链接：`{public_url}/invite/{token}`。
    pub fn invite_url(&self, token: &str) -> String {
        let base = self.notes.page_url("");
        format!("{}/invite/{token}", base.trim_end_matches("/p/"))
    }

    /// 建邀请：角色不能比自己高；expires_in 是秒（None 一直有效）。
    pub async fn create_invite(
        &self,
        workspace_id: &str,
        who: &Principal,
        role: Role,
        expires_in: Option<i64>,
    ) -> Result<Invite> {
        let mine = self.require_manager(workspace_id, who).await?;
        if role > mine {
            return Err(Error::Forbidden("不能邀请比自己权限高的角色".into()));
        }
        if expires_in.is_some_and(|s| s <= 0) {
            return Err(Error::BadRequest("expires_in 要是正数（秒）".into()));
        }
        let now = ids::now_ms();
        let inv = Invite {
            token: ids::new_token()[..32].to_string(),
            workspace_id: workspace_id.into(),
            role,
            created_by: Some(who.user_id.clone()),
            created_at: now,
            expires_at: expires_in.map(|s| now + s * 1000),
            revoked_at: None,
            uses: 0,
        };
        self.storage.create_invite(inv.clone()).await?;
        Ok(inv)
    }

    pub async fn list_invites(&self, workspace_id: &str, who: &Principal) -> Result<Vec<Invite>> {
        self.require_manager(workspace_id, who).await?;
        self.storage.invites(workspace_id).await
    }

    pub async fn revoke_invite(
        &self,
        workspace_id: &str,
        token: &str,
        who: &Principal,
    ) -> Result<()> {
        self.require_manager(workspace_id, who).await?;
        match self.storage.invite(token).await? {
            Some(i) if i.workspace_id == workspace_id && i.revoked_at.is_none() => {
                self.storage.revoke_invite(token).await?;
                Ok(())
            }
            _ => Err(Error::NotFound("没有这个邀请".into())),
        }
    }

    /// 能用的邀请：没有 → NotFound；撤销了、过期了 → Gone（HTTP 410）。
    async fn live_invite(&self, token: &str) -> Result<Invite> {
        let i = self
            .storage
            .invite(token)
            .await?
            .ok_or_else(|| Error::NotFound("邀请链接无效".into()))?;
        if i.revoked_at.is_some() || i.expires_at.is_some_and(|e| e <= ids::now_ms()) {
            return Err(Error::Gone("邀请链接已失效".into()));
        }
        Ok(i)
    }

    pub async fn invite_info(&self, token: &str) -> Result<InviteInfo> {
        let i = self.live_invite(token).await?;
        let w = self
            .storage
            .workspace(&i.workspace_id)
            .await?
            .ok_or_else(|| Error::NotFound("邀请链接无效".into()))?;
        let by = match &i.created_by {
            Some(u) => self
                .storage
                .user(u)
                .await?
                .map(|u| serde_json::json!({ "name": u.name })),
            None => None,
        };
        Ok(InviteInfo {
            workspace: serde_json::json!({ "id": w.id, "name": w.name, "icon": w.icon }),
            role: i.role,
            invited_by: by,
            expires_at: i.expires_at,
            signup: self.auth.mode() == AuthMode::Local && self.signup_mode() != SignupMode::Off,
        })
    }

    /// 接受邀请（已登录）：加进那个工作区；已经是成员的，角色只升不降。返回工作区 id。
    pub async fn accept_invite(&self, token: &str, who: &Principal) -> Result<String> {
        if who.kind == PrincipalKind::Agent {
            return Err(Error::Forbidden("Agent 不能接受邀请".into()));
        }
        let i = self.live_invite(token).await?;
        let cur = self
            .storage
            .member_role(&i.workspace_id, &who.user_id)
            .await?;
        if cur.is_none_or(|r| r < i.role) {
            self.storage
                .set_member(&i.workspace_id, &who.user_id, i.role)
                .await?;
        }
        self.storage.use_invite(token).await?;
        Ok(i.workspace_id)
    }

    /// 注册（本地登录模式）：建用户、登录（返回会话令牌）。invite 模式要带有效的邀请；有邀请就加进邀请的工作区，
    /// 没有（open 模式）就加进默认工作区。
    pub async fn signup(
        &self,
        email: &str,
        password: &str,
        name: &str,
        invite: Option<&str>,
    ) -> Result<(String, User)> {
        if self.auth.mode() != AuthMode::Local {
            return Err(Error::BadRequest("当前登录方式不支持注册".into()));
        }
        let inv = match invite.filter(|t| !t.is_empty()) {
            Some(t) => Some(self.live_invite(t).await?),
            None => None,
        };
        match (self.signup_mode(), &inv) {
            (SignupMode::Off, _) => return Err(Error::Forbidden("这个站点不开放注册".into())),
            (SignupMode::Invite, None) => return Err(Error::Forbidden("要凭邀请链接注册".into())),
            _ => {}
        }
        let name = if name.trim().is_empty() {
            String::new()
        } else {
            clean_name(name)?
        };
        let email = email.trim();
        if email.is_empty() || !email.contains('@') {
            return Err(Error::BadRequest("邮箱格式不对".into()));
        }
        if password.chars().count() < 8 {
            return Err(Error::BadRequest("密码至少 8 位".into()));
        }
        let user = self
            .storage
            .create_user(NewUser {
                name: if name.is_empty() {
                    email.split('@').next().unwrap_or(email).into()
                } else {
                    name
                },
                email: Some(email.into()),
                password_hash: Some(hash_password(password).await?),
                external_id: None,
                kind: PrincipalKind::User,
            })
            .await
            .map_err(|e| match e {
                Error::Conflict(_) => Error::Conflict(format!("邮箱 {email} 已经注册过")),
                e => e,
            })?;
        match &inv {
            Some(i) => {
                self.storage
                    .set_member(&i.workspace_id, &user.id, i.role)
                    .await?;
                self.storage.use_invite(&i.token).await?;
            }
            None => {
                crate::join_default(&*self.storage, &self.default_workspace.id, &user.id).await?;
            }
        }
        let token = ids::new_token();
        let expires = ids::now_ms() + SESSION_DAYS * 24 * 3600 * 1000;
        self.storage
            .create_session(&ids::hash_token(&token), &user.id, expires)
            .await?;
        Ok((token, user))
    }

    /// 改成员的角色：要是管理员；所有者只有所有者能动；不能给比自己高的角色；至少留一个所有者。
    pub async fn set_member_role(
        &self,
        workspace_id: &str,
        user_id: &str,
        role: Role,
        who: &Principal,
    ) -> Result<()> {
        let mine = self.require_manager(workspace_id, who).await?;
        let cur = self
            .storage
            .member_role(workspace_id, user_id)
            .await?
            .ok_or_else(|| Error::NotFound("不是这个工作区的成员".into()))?;
        if (cur == Role::Owner || role == Role::Owner) && mine != Role::Owner {
            return Err(Error::Forbidden("只有所有者能改所有者".into()));
        }
        if role > mine {
            return Err(Error::Forbidden("不能给比自己权限高的角色".into()));
        }
        if cur == Role::Owner && role != Role::Owner && self.owners(workspace_id).await? <= 1 {
            return Err(Error::BadRequest("工作区至少要有一个所有者".into()));
        }
        self.storage.set_member(workspace_id, user_id, role).await
    }

    async fn owners(&self, workspace_id: &str) -> Result<usize> {
        Ok(self
            .storage
            .members(workspace_id)
            .await?
            .iter()
            .filter(|(u, r)| *r == Role::Owner && u.kind == PrincipalKind::User)
            .count())
    }

    /// 移出成员（自己退出也走这里）。所有者只有所有者能移；最后一个所有者不能走。
    pub async fn remove_member(
        &self,
        workspace_id: &str,
        user_id: &str,
        who: &Principal,
    ) -> Result<()> {
        let cur = self
            .storage
            .member_role(workspace_id, user_id)
            .await?
            .ok_or_else(|| Error::NotFound("不是这个工作区的成员".into()))?;
        if user_id != who.user_id {
            let mine = self.require_manager(workspace_id, who).await?;
            if cur == Role::Owner && mine != Role::Owner {
                return Err(Error::Forbidden("只有所有者能移出所有者".into()));
            }
        }
        if cur == Role::Owner && self.owners(workspace_id).await? <= 1 {
            return Err(Error::BadRequest("工作区至少要有一个所有者".into()));
        }
        if self
            .storage
            .user(user_id)
            .await?
            .is_some_and(|u| u.kind == PrincipalKind::Agent)
        {
            return Err(Error::BadRequest("Agent 在 Agent 设置里撤销".into()));
        }
        self.storage.remove_member(workspace_id, user_id).await?;
        Ok(())
    }

    pub async fn update_workspace(
        &self,
        workspace_id: &str,
        who: &Principal,
        name: Option<String>,
        icon: Option<Option<String>>,
    ) -> Result<Workspace> {
        self.require_manager(workspace_id, who).await?;
        let name = name.map(|n| clean_name(&n)).transpose()?;
        let icon = icon.map(|i| {
            i.map(|x| x.trim().chars().take(64).collect::<String>())
                .filter(|x| !x.is_empty())
        });
        let w = self
            .storage
            .update_workspace(workspace_id, name, icon)
            .await?;
        self.events.emit(crate::Event {
            workspace_id: workspace_id.into(),
            name: "workspace.updated",
            data: serde_json::json!({ "id": w.id, "name": w.name, "icon": w.icon }),
            user_id: None,
        });
        Ok(w)
    }

    /// 改自己的资料：名字、头像（上传后的文件网址）、语言（zh / en）、时区（IANA 名字）。
    pub async fn update_me(&self, who: &Principal, mut p: PrefsPatch) -> Result<()> {
        if let Some(n) = &p.name {
            p.name = Some(clean_name(n)?);
        }
        if let Some(l) = p.locale.take() {
            p.locale = Some(crate::i18n::check_locale(l)?);
        }
        if let Some(Some(tz)) = &p.timezone
            && crate::tz::parse(tz).is_none()
        {
            return Err(Error::BadRequest(format!(
                "不认识的时区 {tz}（要写 IANA 名字，如 Asia/Shanghai）"
            )));
        }
        if let Some(Some(a)) = &p.avatar
            && !(a.starts_with('/') || a.starts_with("https://") || a.starts_with("http://"))
        {
            return Err(Error::BadRequest("头像要是网址".into()));
        }
        self.storage.update_user(&who.user_id, p).await
    }

    /// 改密码：要先给对现在的密码；别处的登录都退出（keep 是这次请求的会话令牌哈希）。
    pub async fn change_password(
        &self,
        who: &Principal,
        current: &str,
        new: &str,
        keep: Option<&str>,
    ) -> Result<()> {
        if self.auth.mode() != AuthMode::Local || who.kind == PrincipalKind::Agent {
            return Err(Error::BadRequest("当前登录方式不支持改密码".into()));
        }
        let hash = self.storage.password_hash(&who.user_id).await?;
        let ok = match &hash {
            Some(h) => verify_password(current, h).await,
            None => false,
        };
        if !ok {
            return Err(Error::Forbidden("现在的密码不对".into()));
        }
        if new.chars().count() < 8 {
            return Err(Error::BadRequest("密码至少 8 位".into()));
        }
        self.storage
            .set_password(&who.user_id, &hash_password(new).await?)
            .await?;
        self.storage.delete_user_sessions(&who.user_id, keep).await
    }
}
