//! GitHub 仓库 / issue / PR：不带 token 的 REST API（每个 IP 每小时 60 次），超了就退到网页 meta。
//! 设了 `OAD_GITHUB_TOKEN` 就带上（5000 次 / 小时）。

use serde_json::Value;
use url::Url;

use super::{Log, jstr, ju64};
use crate::error::Result;
use crate::fetch::{Fetcher, RequestOpts};
use crate::html;
use crate::model::{
    Author, Embed, GithubIssue, GithubLabel, GithubRepo, Image, Kind, Provider, Source, Unfurl,
};
use crate::providers::generic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GhRef {
    Repo {
        owner: String,
        repo: String,
    },
    Issue {
        owner: String,
        repo: String,
        number: u64,
        pull: bool,
    },
}

/// 第一段是这些的不是用户名。
const RESERVED: &[&str] = &[
    "about",
    "apps",
    "orgs",
    "settings",
    "marketplace",
    "topics",
    "features",
    "sponsors",
    "login",
    "logout",
    "join",
    "explore",
    "trending",
    "collections",
    "enterprise",
    "pricing",
    "notifications",
    "pulls",
    "issues",
    "search",
    "users",
    "new",
    "codespaces",
    "security",
    "site",
    "readme",
    "events",
    "organizations",
    "dashboard",
    "account",
    "customer-stories",
    "team",
    "contact",
    "blog",
    "resources",
    "solutions",
    "stars",
    "watching",
    "copilot",
    "models",
];

pub fn parse(url: &Url) -> Option<GhRef> {
    let host = url.host_str()?.to_ascii_lowercase();
    if host != "github.com" && host != "www.github.com" {
        return None;
    }
    let segs: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    let owner = segs.first()?;
    if RESERVED.contains(&owner.to_ascii_lowercase().as_str()) {
        return None;
    }
    let repo = segs.get(1)?.trim_end_matches(".git");
    if repo.is_empty() {
        return None;
    }
    let (owner, repo) = (owner.to_string(), repo.to_string());
    match segs.get(2).copied() {
        None
        | Some("tree" | "blob" | "commits" | "releases" | "tags" | "wiki" | "actions" | "pulls") => {
            Some(GhRef::Repo { owner, repo })
        }
        Some(kind @ ("issues" | "pull")) => match segs.get(3) {
            // 只有 /issues 列表页：当仓库
            None => Some(GhRef::Repo { owner, repo }),
            Some(n) => Some(GhRef::Issue {
                owner,
                repo,
                number: n.parse().ok()?,
                pull: kind == "pull",
            }),
        },
        _ => None,
    }
}

fn api_opts() -> RequestOpts {
    let mut o = RequestOpts::json().header(
        reqwest::header::HeaderName::from_static("x-github-api-version"),
        "2022-11-28",
    );
    o.accept = Some("application/vnd.github+json");
    if let Ok(t) = std::env::var("OAD_GITHUB_TOKEN")
        && !t.trim().is_empty()
    {
        o = o.header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", t.trim()),
        );
    }
    o
}

async fn api(f: &Fetcher, path: &str) -> Result<Value> {
    let u = Url::parse(&format!("https://api.github.com{path}")).unwrap();
    f.request(&u, api_opts()).await?.ok()?.json()
}

fn user_author(v: &Value) -> Option<Author> {
    let login = jstr(v, &["login"])?;
    Some(Author {
        name: Some(login.clone()),
        url: jstr(v, &["html_url"]),
        avatar: jstr(v, &["avatar_url"]),
        handle: Some(login),
    })
}

fn og_image(path: &str) -> Image {
    // GitHub 自己生成的社交卡片图，地址里第一段随便填（用来破缓存）
    Image::sized(
        format!("https://opengraph.githubassets.com/1/{path}"),
        Some(1200),
        Some(600),
    )
}

fn base(input: &Url, kind: Kind, source: Source) -> Unfurl {
    let mut u = Unfurl::new(kind, Provider::Github, source, input.as_str());
    u.site_name = Some("GitHub".into());
    u.favicon = Some("https://github.githubassets.com/favicons/favicon.svg".into());
    u
}

fn repo_card(input: &Url, owner: &str, repo: &str, v: &Value) -> Unfurl {
    let mut u = base(input, Kind::Repo, Source::Api);
    let full = jstr(v, &["full_name"]).unwrap_or_else(|| format!("{owner}/{repo}"));
    u.canonical_url = jstr(v, &["html_url"]);
    u.title = Some(full.clone());
    u.description = jstr(v, &["description"]).map(|d| html::truncate(&d, 500));
    u.author = v.get("owner").and_then(user_author);
    u.thumbnail = Some(og_image(&full));
    u.published_at = jstr(v, &["created_at"]);
    let (o, r) = full.split_once('/').unwrap_or((owner, repo));
    u.embed = Some(Embed::GithubRepo(GithubRepo {
        owner: o.to_string(),
        repo: r.to_string(),
        stars: ju64(v, &["stargazers_count"]),
        forks: ju64(v, &["forks_count"]),
        language: jstr(v, &["language"]),
        license: jstr(v, &["license", "spdx_id"]).filter(|l| l != "NOASSERTION"),
        topics: v
            .get("topics")
            .and_then(|t| t.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        pushed_at: jstr(v, &["pushed_at"]),
        archived: v.get("archived").and_then(|a| a.as_bool()).unwrap_or(false),
    }));
    u
}

fn issue_card(
    input: &Url,
    owner: &str,
    repo: &str,
    number: u64,
    v: &Value,
    pr: Option<&Value>,
) -> Unfurl {
    let is_pr = pr.is_some()
        || v.get("pull_request").is_some_and(|p| !p.is_null())
        || v.get("merged").is_some();
    let mut u = base(
        input,
        if is_pr {
            Kind::PullRequest
        } else {
            Kind::Issue
        },
        Source::Api,
    );
    u.canonical_url = jstr(v, &["html_url"]);
    u.title = jstr(v, &["title"]).map(|t| format!("{t} · #{number}"));
    u.description = jstr(v, &["body"]).map(|b| html::truncate(&b, 300));
    u.author = v.get("user").and_then(user_author);
    u.published_at = jstr(v, &["created_at"]);
    u.thumbnail = Some(og_image(&format!(
        "{owner}/{repo}/{}/{number}",
        if is_pr { "pull" } else { "issues" }
    )));
    let src = pr.unwrap_or(v);
    let merged_at = jstr(src, &["merged_at"]).or_else(|| jstr(v, &["pull_request", "merged_at"]));
    let draft = src.get("draft").and_then(|d| d.as_bool()).unwrap_or(false);
    let state = jstr(v, &["state"]).map(|s| {
        if merged_at.is_some() {
            "merged".to_string()
        } else if draft && s == "open" {
            "draft".to_string()
        } else {
            s
        }
    });
    u.embed = Some(Embed::GithubIssue(GithubIssue {
        owner: owner.to_string(),
        repo: repo.to_string(),
        number,
        is_pull_request: is_pr,
        state,
        state_reason: jstr(v, &["state_reason"]),
        labels: v
            .get("labels")
            .and_then(|l| l.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|l| {
                        Some(GithubLabel {
                            name: jstr(l, &["name"])?,
                            color: jstr(l, &["color"]),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        comments: ju64(v, &["comments"]),
        closed_at: jstr(v, &["closed_at"]),
        merged_at,
        additions: pr.and_then(|p| ju64(p, &["additions"])),
        deletions: pr.and_then(|p| ju64(p, &["deletions"])),
        changed_files: pr.and_then(|p| ju64(p, &["changed_files"])),
        base_ref: pr.and_then(|p| jstr(p, &["base", "ref"])),
        head_ref: pr.and_then(|p| jstr(p, &["head", "label"])),
    }));
    u
}

pub async fn unfurl(f: &Fetcher, input: &Url, r: GhRef, log: &mut Log) -> Result<Unfurl> {
    let (kind, page) = match &r {
        GhRef::Repo { owner, repo } => {
            if let Some(v) = log
                .step("github.api", api(f, &format!("/repos/{owner}/{repo}")))
                .await
            {
                return Ok(repo_card(input, owner, repo, &v));
            }
            (Kind::Repo, format!("https://github.com/{owner}/{repo}"))
        }
        GhRef::Issue {
            owner,
            repo,
            number,
            pull,
        } => {
            let path = if *pull {
                format!("/repos/{owner}/{repo}/pulls/{number}")
            } else {
                format!("/repos/{owner}/{repo}/issues/{number}")
            };
            if let Some(v) = log.step("github.api", api(f, &path)).await {
                // pulls 接口自带 additions / merged / draft 等，直接当 PR 用
                return Ok(if *pull {
                    issue_card(input, owner, repo, *number, &v, Some(&v))
                } else {
                    issue_card(input, owner, repo, *number, &v, None)
                });
            }
            // /pull/N 其实是 issue（GitHub 网页会跳转）：用 issues 接口再试一次
            if *pull
                && log
                    .attempts
                    .last()
                    .is_some_and(|a| a.error.as_deref() == Some("not found"))
                && let Some(v) = log
                    .step(
                        "github.api.issue",
                        api(f, &format!("/repos/{owner}/{repo}/issues/{number}")),
                    )
                    .await
            {
                return Ok(issue_card(input, owner, repo, *number, &v, None));
            }
            (
                if *pull {
                    Kind::PullRequest
                } else {
                    Kind::Issue
                },
                format!(
                    "https://github.com/{owner}/{repo}/{}/{number}",
                    if *pull { "pull" } else { "issues" }
                ),
            )
        }
    };
    // 限流或接口出错：GitHub 网页的 OG 也很完整
    let page = Url::parse(&page).unwrap();
    if let Some(mut g) = log
        .step("html", generic::unfurl(f, &page, &mut Log::default()))
        .await
    {
        g.provider = Provider::Github;
        g.kind = kind;
        g.url = input.to_string();
        return Ok(g);
    }
    Err(log.all_failed())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Option<GhRef> {
        parse(&Url::parse(s).unwrap())
    }

    #[test]
    fn urls() {
        let repo = |o: &str, r: &str| {
            Some(GhRef::Repo {
                owner: o.into(),
                repo: r.into(),
            })
        };
        assert_eq!(
            p("https://github.com/rust-lang/rust"),
            repo("rust-lang", "rust")
        );
        assert_eq!(
            p("https://github.com/rust-lang/rust.git"),
            repo("rust-lang", "rust")
        );
        assert_eq!(
            p("https://www.github.com/rust-lang/rust/tree/master/library"),
            repo("rust-lang", "rust")
        );
        assert_eq!(
            p("https://github.com/rust-lang/rust/blob/master/README.md"),
            repo("rust-lang", "rust")
        );
        assert_eq!(
            p("https://github.com/rust-lang/rust/issues"),
            repo("rust-lang", "rust")
        );
        assert_eq!(
            p("https://github.com/rust-lang/rust/issues/12345#issuecomment-1"),
            Some(GhRef::Issue {
                owner: "rust-lang".into(),
                repo: "rust".into(),
                number: 12345,
                pull: false
            })
        );
        assert_eq!(
            p("https://github.com/rust-lang/rust/pull/99/files"),
            Some(GhRef::Issue {
                owner: "rust-lang".into(),
                repo: "rust".into(),
                number: 99,
                pull: true
            })
        );
        assert!(p("https://github.com/rust-lang").is_none());
        assert!(p("https://github.com/settings/profile").is_none());
        assert!(p("https://github.com/orgs/rust-lang/people").is_none());
        assert!(p("https://github.com/rust-lang/rust/discussions/1").is_none());
        assert!(p("https://gist.github.com/a/b").is_none());
    }
}
