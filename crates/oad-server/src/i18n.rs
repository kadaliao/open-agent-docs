//! 服务器的双语：错误信息按调用者的语言（设过的 > Accept-Language > 中文）翻成英文；服务器自己的话的表 [`SERVER`]。
use crate::AppState;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, header};
use axum::middleware::Next;
use axum::response::Response;
use oad_core::i18n::{CORE, DB_ENGINE, Lang, Table, t_in, translate};

/// 服务器这一层的话：接口的错误、分享页面上的字。
pub static SERVER: Table = &[
    // 接口的错误
    ("没有登录", "Not signed in"),
    ("服务器内部错误", "Internal server error"),
    ("没有这个接口", "No such endpoint"),
    (
        "失败次数太多，稍后再试",
        "Too many failed attempts, try again later",
    ),
    ("不支持的 format={f}", "Unsupported format={f}"),
    (
        "limit 要是非负整数：{l}",
        "limit must be a non-negative integer: {l}",
    ),
    ("base 不是合法的版本", "base is not a valid version"),
    ("doc 的 type 要是 \"doc\"", "doc.type must be \"doc\""),
    (
        "要给 markdown 或 doc 其中一个",
        "Provide either markdown or doc",
    ),
    (
        "scope 只能是 read / write / admin",
        "scope must be read / write / admin",
    ),
    (
        "resolved 只能是 false / true / all，不是 {x}",
        "resolved must be false / true / all, not {x}",
    ),
    ("{k} 要是整数", "{k} must be an integer"),
    ("要给 ids 或 upto", "Provide ids or upto"),
    ("要给 ids 或 all: true", "Provide ids or all: true"),
    ("没有 file 字段", "Missing file field"),
    ("要有 file 字段", "A file field is required"),
    ("没有这张图", "Image not found"),
    (
        "签名地址过期了，重新取一个",
        "This signed URL has expired; get a new one",
    ),
    ("签名不对", "Invalid signature"),
    ("一次最多签 500 个", "At most 500 URLs per request"),
    ("要给 url", "url is required"),
    (
        "to 要是版本号或 current",
        "to must be a version number or current",
    ),
    (
        "role 只能是 viewer、editor、admin、owner",
        "role must be viewer, editor, admin or owner",
    ),
    (
        "Host 不在允许列表里（设 OAD_ALLOWED_HOSTS 或 OAD_PUBLIC_URL）",
        "Host is not allowed (set OAD_ALLOWED_HOSTS or OAD_PUBLIC_URL)",
    ),
    (
        "Origin 不在允许列表里（嵌入时设 OAD_ALLOWED_ORIGINS）",
        "Origin is not allowed (set OAD_ALLOWED_ORIGINS when embedding)",
    ),
    // 分享页面
    ("服务器出错了", "Something went wrong on the server"),
    ("无标题", "Untitled"),
    ("{n} 行", "{n} rows"),
    ("（没有分享的页面）", "(Page not shared)"),
    ("播放", "Play"),
];

/// 服务器的一句话。
pub fn t(lang: Lang, zh: &str, args: &[(&str, &str)]) -> String {
    t_in(&[SERVER, CORE, DB_ENGINE], lang, zh, args)
}

/// 接口的错误 `{ error: { code, message } }`：message 按调用者的语言翻。
pub async fn errors(State(s): State<AppState>, req: Request, next: Next) -> Response {
    let headers = req.headers().clone();
    let uri = req.uri().clone();
    let res = next.run(req).await;
    let st = res.status();
    if !(st.is_client_error() || st.is_server_error())
        || !res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("application/json"))
    {
        return res;
    }
    let mut parts = Request::builder()
        .uri(uri)
        .body(())
        .map(|r| r.into_parts().0)
        .ok();
    if let Some(p) = &mut parts {
        p.headers = headers.clone();
    }
    let who = match &parts {
        Some(p) => crate::api::authenticate(&s, p).await.ok().flatten(),
        None => None,
    };
    let lang = crate::api::req_lang(&s, who.as_ref(), &headers).await;
    if lang == Lang::Zh {
        return res;
    }
    let (mut head, body) = res.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, 1 << 20).await else {
        return Response::from_parts(head, Body::empty());
    };
    let mut v: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return Response::from_parts(head, Body::from(bytes)),
    };
    if let Some(m) = v["error"]["message"].as_str() {
        v["error"]["message"] = translate(&[SERVER, CORE, DB_ENGINE], lang, m).into();
    }
    let out = serde_json::to_vec(&v).unwrap_or_default();
    head.headers.remove(header::CONTENT_LENGTH);
    head.headers
        .insert(header::CONTENT_LANGUAGE, HeaderValue::from_static("en"));
    Response::from_parts(head, Body::from(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(s: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut rest = s;
        while let Some(i) = rest.find('{') {
            let after = &rest[i + 1..];
            match after.find('}') {
                Some(j) if j > 0 && after[..j].chars().all(|c| c.is_alphanumeric() || c == '_') => {
                    out.push(&after[..j]);
                    rest = &after[j + 1..];
                }
                _ => rest = after,
            }
        }
        out.sort_unstable();
        out
    }

    fn cjk(s: &str) -> bool {
        s.chars().any(|c| matches!(c, '\u{3000}'..='\u{303f}' | '\u{3400}'..='\u{9fff}' | '\u{ff00}'..='\u{ffef}'))
    }

    // 中英文变量一样、中文不重复、英文里没有中文
    #[test]
    fn server_table() {
        let mut seen = std::collections::HashSet::new();
        for (zh, en) in SERVER {
            assert!(seen.insert(*zh), "重复：{zh}");
            assert_eq!(names(zh), names(en), "变量对不上：{zh}");
            assert!(!cjk(en), "英文里有中文：{en}");
            assert!(!zh.contains("}{"), "两个变量挨着认不出：{zh}");
        }
    }
}
