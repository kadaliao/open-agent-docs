//! 双语：接口的错误按 Accept-Language / 用户设的语言；分享页面按浏览器的语言；恢复版本的名字按恢复的人的语言。
mod common;
use common::*;
use oad_core::AuthConfig;
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn errors_follow_locale() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let (st, v) = json(c.get(s.api("/pages/nope1234abcd"))).await;
    assert_eq!(st, 404);
    assert_eq!(v["error"]["message"], "页面 nope1234abcd 不存在");
    let (_, v) = json(
        c.get(s.api("/pages/nope1234abcd"))
            .header("accept-language", "en-US,en;q=0.9"),
    )
    .await;
    assert_eq!(v["error"]["message"], "Page nope1234abcd not found");
    assert_eq!(v["error"]["code"], "not_found");
    // 用户设了语言：不看浏览器的
    let (st, _) = json(c.patch(s.api("/me")).json(&json!({ "locale": "en" }))).await;
    assert_eq!(st, 200);
    let (_, v) = json(
        c.get(s.api("/pages/nope1234abcd"))
            .header("accept-language", "zh-CN"),
    )
    .await;
    assert_eq!(v["error"]["message"], "Page nope1234abcd not found");
    // 恢复版本的名字
    let page = page_with(&s, &c, "版本", "一").await;
    let (_, m) = json(
        c.post(s.api(&format!("/pages/{page}/versions")))
            .json(&json!({})),
    )
    .await;
    let (_, r) = json(c.post(s.api(&format!("/pages/{page}/versions/{}/restore", m["id"])))).await;
    assert_eq!(
        r["version"]["label"],
        format!("Restored version {}", m["id"])
    );
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn share_page_language() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let (_, p) = json(
        c.post(s.api(&format!("/workspaces/{}/pages", s.ws_id())))
            .json(&json!({})),
    )
    .await;
    let id = p["id"].as_str().unwrap();
    let (_, sh) = json(
        c.post(s.api(&format!("/pages/{id}/share")))
            .json(&json!({})),
    )
    .await;
    let url = format!("http://{}/s/{}", s.addr, sh["token"].as_str().unwrap());
    let zh = c.get(&url).send().await.unwrap().text().await.unwrap();
    assert!(zh.contains("<html lang=\"zh\"") && zh.contains("无标题"));
    let en = c
        .get(&url)
        .header("accept-language", "en")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(en.contains("<html lang=\"en\"") && en.contains("Untitled") && !en.contains("无标题"));
    let gone = c
        .get(format!("http://{}/s/nope", s.addr))
        .header("accept-language", "en")
        .send()
        .await
        .unwrap();
    assert_eq!(gone.status().as_u16(), 404);
    let body = gone.text().await.unwrap();
    assert!(
        !body.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
        "{body}"
    );
    s.stop().await;
}

/// 数据库引擎（oad-db）的报错也翻。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn db_engine_errors() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let (_, db) = json(c.post(s.api(&format!("/workspaces/{}/databases", s.ws_id()))).json(&json!({
        "schema": { "props": [{ "name": "名称", "type": "title" }, { "name": "价格", "type": "number" }] }
    })))
    .await;
    let db = db["page"]["id"].as_str().unwrap().to_string();
    let (_, add) = json(
        c.post(s.api(&format!("/databases/{db}/rows")))
            .json(&json!({ "rows": [{ "props": {} }] })),
    )
    .await;
    let row = add["ids"][0].as_str().unwrap().to_string();
    let body = json!({ "ops": [{ "row_id": row, "set": { "价格": "很贵" } }] });
    let (st, zh) = json(c.patch(s.api(&format!("/databases/{db}/rows"))).json(&body)).await;
    assert_eq!(st, 400);
    let (_, en) = json(
        c.patch(s.api(&format!("/databases/{db}/rows")))
            .header("accept-language", "en")
            .json(&body),
    )
    .await;
    let m = en["error"]["message"].as_str().unwrap();
    assert_ne!(m, zh["error"]["message"].as_str().unwrap());
    assert!(
        m.contains("价格") && m.contains("很贵"),
        "用户写的名字和值原样：{m}"
    );
    let template: String = m.replace("价格", "").replace("很贵", "");
    assert!(
        !template
            .chars()
            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
        "{m}"
    );
    s.stop().await;
}
