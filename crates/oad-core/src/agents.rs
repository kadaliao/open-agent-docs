//! Agent：属于一个工作区，用 `oad_` 开头的 token 认证，按 scope 读写；admin 还能管本工作区别的 Agent、收所有 Agent 的通知。
use crate::model::*;
use crate::{AuthRequest, Core, Error, Result, ids};
use serde::Serialize;

pub const TOKEN_PREFIX: &str = "oad_";
const SECRET_PREFIX: &str = "whsec_";

fn random_b62(n: usize) -> String {
    const B62: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    // 拒绝采样，不偏
    let mut out = String::with_capacity(n);
    while out.len() < n {
        let b: u8 = rand::random();
        if b < 248 {
            out.push(B62[(b % 62) as usize] as char);
        }
    }
    out
}

/// 新 token：`oad_` + 43 位 base62（约 256 位随机）。
pub fn new_agent_token() -> String {
    format!("{TOKEN_PREFIX}{}", random_b62(43))
}

pub fn new_webhook_secret() -> String {
    format!("{SECRET_PREFIX}{}", random_b62(43))
}

/// Agent 列表里的一项：加上没处理的通知数和最近的 webhook 错误。
#[derive(Serialize, Clone, Debug)]
pub struct AgentView {
    #[serde(flatten)]
    pub agent: Agent,
    pub pending: i64,
    pub webhook_error: Option<String>,
}

fn check_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 40 {
        return Err(Error::BadRequest("名字要 1 到 40 个字".into()));
    }
    if n.chars()
        .any(|c| c.is_whitespace() || "@[]()<>\"'`,，".contains(c))
    {
        return Err(Error::BadRequest(
            "名字里不能有空白和 @ [ ] ( ) < > 引号 逗号（@ 的时候要用）".into(),
        ));
    }
    Ok(n.to_string())
}

impl Core {
    /// 认证：`Authorization: Bearer oad_...` 是 Agent；别的交给登录方式（AuthProvider）。
    /// 给了 Agent token 但不对时不退到别的方式（不登录模式下也不会变成本机用户）。
    pub async fn authenticate(&self, req: &AuthRequest<'_>) -> Result<Option<Principal>> {
        let agent_token = req
            .session_tokens()
            .into_iter()
            .find(|t| t.starts_with(TOKEN_PREFIX));
        if let Some(t) = agent_token {
            let Some(a) = self.storage.agent_by_token(&ids::hash_token(t)).await? else {
                return Ok(None);
            };
            self.touch_agent(&a.id).await;
            return Ok(Some(Principal {
                user_id: a.id,
                name: a.name,
                kind: PrincipalKind::Agent,
            }));
        }
        self.auth.authenticate(req).await
    }

    /// 最后使用时间，一分钟最多写一次库。
    async fn touch_agent(&self, id: &str) {
        let now = ids::now_ms();
        {
            let mut t = self.touched.lock().unwrap();
            if t.get(id).is_some_and(|&last| now - last < 60_000) {
                return;
            }
            t.insert(id.to_string(), now);
        }
        let _ = self.storage.touch_agent(id, now).await;
    }

    /// 调用者如果是 Agent，取出来（撤销了的也返回）。
    pub async fn agent_of(&self, who: &Principal) -> Result<Option<Agent>> {
        if who.kind != PrincipalKind::Agent {
            return Ok(None);
        }
        self.storage.agent(&who.user_id).await
    }

    /// 能不能管这个工作区的 Agent：owner / editor 能；本工作区 admin 权限的 Agent 能（target 是它自己时不能）。
    async fn require_agent_admin(
        &self,
        workspace_id: &str,
        who: &Principal,
        target: Option<&str>,
    ) -> Result<()> {
        match self.agent_of(who).await? {
            Some(a) => {
                if a.workspace_id != workspace_id || a.scope != Scope::Admin || !a.active() {
                    return Err(Error::Forbidden(
                        "只有 admin 权限的 Agent 能管 Agent".into(),
                    ));
                }
                if target == Some(a.id.as_str()) {
                    return Err(Error::Forbidden("Agent 不能管自己".into()));
                }
                Ok(())
            }
            None => {
                self.require(workspace_id, who, true).await?;
                Ok(())
            }
        }
    }

    /// 工作区里的 Agent（成员都能看，@ 的时候要用）。
    pub async fn list_agents(&self, workspace_id: &str, who: &Principal) -> Result<Vec<AgentView>> {
        self.require(workspace_id, who, false).await?;
        let stats = self.storage.agent_stats(workspace_id).await?;
        Ok(self
            .storage
            .agents(workspace_id)
            .await?
            .into_iter()
            .map(|a| {
                let (pending, webhook_error) = stats
                    .iter()
                    .find(|s| s.0 == a.id)
                    .map(|s| (s.1, s.2.clone()))
                    .unwrap_or((0, None));
                AgentView {
                    agent: a,
                    pending,
                    webhook_error,
                }
            })
            .collect())
    }

    /// 工作区里的人（不含 Agent）和 Agent（@ 的时候选）。成员都能看。
    pub async fn members(
        &self,
        workspace_id: &str,
        who: &Principal,
    ) -> Result<(Vec<(User, Role)>, Vec<AgentView>)> {
        let agents = self.list_agents(workspace_id, who).await?;
        let mut people: Vec<(User, Role)> = self
            .storage
            .members(workspace_id)
            .await?
            .into_iter()
            .filter(|(u, _)| u.kind == PrincipalKind::User)
            .collect();
        people.sort_by(|a, b| (a.0.created_at, &a.0.id).cmp(&(b.0.created_at, &b.0.id)));
        Ok((people, agents))
    }

    /// 建 Agent，返回 (Agent, token, webhook_secret)；token 和 secret 只有这一次能看到。
    pub async fn create_agent(
        &self,
        workspace_id: &str,
        who: &Principal,
        name: &str,
        about: Option<String>,
        scope: Scope,
        webhook: Option<String>,
    ) -> Result<(Agent, String, String)> {
        self.require_agent_admin(workspace_id, who, None).await?;
        let name = check_name(name)?;
        let webhook = webhook.filter(|w| !w.trim().is_empty());
        if let Some(w) = &webhook {
            self.notes.check_webhook(w)?;
        }
        let token = new_agent_token();
        let secret = new_webhook_secret();
        let agent = self
            .storage
            .create_agent(NewAgent {
                workspace_id: workspace_id.into(),
                name,
                about: about.unwrap_or_default(),
                scope,
                webhook,
                webhook_secret: secret.clone(),
                token_hash: ids::hash_token(&token),
                created_by: Some(who.user_id.clone()),
            })
            .await?;
        Ok((agent, token, secret))
    }

    async fn managed_agent(&self, id: &str, who: &Principal) -> Result<Agent> {
        let a = self
            .storage
            .agent(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Agent {id} 不存在")))?;
        self.require_agent_admin(&a.workspace_id, who, Some(id))
            .await?;
        Ok(a)
    }

    pub async fn update_agent(
        &self,
        id: &str,
        who: &Principal,
        mut patch: AgentPatch,
    ) -> Result<Agent> {
        let a = self.managed_agent(id, who).await?;
        if !a.active() {
            return Err(Error::BadRequest("Agent 已撤销".into()));
        }
        if let Some(n) = &patch.name {
            patch.name = Some(check_name(n)?);
        }
        if let Some(Some(w)) = &patch.webhook {
            if w.trim().is_empty() {
                patch.webhook = Some(None);
            } else {
                self.notes.check_webhook(w)?;
            }
        }
        if let Some(l) = patch.locale.take() {
            let l = crate::i18n::check_locale(l)?;
            self.storage
                .update_user(
                    id,
                    PrefsPatch {
                        locale: Some(l),
                        ..Default::default()
                    },
                )
                .await?;
        }
        self.storage.update_agent(id, patch).await
    }

    pub async fn revoke_agent(&self, id: &str, who: &Principal) -> Result<Agent> {
        self.managed_agent(id, who).await?;
        self.storage.revoke_agent(id).await
    }

    /// 重发 token：旧的立即失效。
    pub async fn rotate_agent_token(&self, id: &str, who: &Principal) -> Result<String> {
        let a = self.managed_agent(id, who).await?;
        if !a.active() {
            return Err(Error::BadRequest("Agent 已撤销".into()));
        }
        let token = new_agent_token();
        self.storage
            .set_agent_token(id, &ids::hash_token(&token))
            .await?;
        Ok(token)
    }

    pub async fn rotate_webhook_secret(&self, id: &str, who: &Principal) -> Result<String> {
        self.managed_agent(id, who).await?;
        let s = new_webhook_secret();
        self.storage.set_webhook_secret(id, &s).await?;
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn tokens() {
        let t = super::new_agent_token();
        assert!(t.starts_with("oad_") && t.len() == 47);
        assert!(super::check_name("写作助手").is_ok());
        assert!(super::check_name("有 空格").is_err());
        assert!(super::check_name("a@b").is_err());
    }
}
