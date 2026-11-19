//! 搜索：中文按词、英文词干、标题优先、片段高亮、工作区和种类过滤、增量（改了的块、删页）、数据库的行、启动时只补漏掉的。
mod common;
use common::*;
use oad_core::AuthConfig;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

async fn search(s: &Server, c: &reqwest::Client, q: &str, kind: Option<&str>) -> Vec<Value> {
    let mut url = format!("/workspaces/{}/search?q={}", s.ws_id(), urlencode(q));
    if let Some(k) = kind {
        url.push_str(&format!("&kind={k}"));
    }
    let (st, v) = json(c.get(s.api(&url))).await;
    assert_eq!(st, 200, "{v}");
    v["results"].as_array().unwrap().clone()
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// 等索引跟上：页面存好、索引处理完。
async fn settle(s: &Server, pages: &[&str]) {
    for p in pages {
        let _ = s.core.hub.flush(p).await;
    }
    s.core.search.flush().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pages_rows_and_incremental() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let a = page_with(
        &s,
        &c,
        "季度计划",
        "我们下个季度要完成**搜索**功能。\n\n第二段讲预算。",
    )
    .await;
    let b = page_with(
        &s,
        &c,
        "Engine notes",
        "The search engine is running fast.\n\n另外一段提到计划。",
    )
    .await;
    settle(&s, &[&a, &b]).await;

    let r = search(&s, &c, "计划", None).await;
    assert_eq!(r[0]["page_id"], a.as_str(), "标题命中排前面：{r:?}");
    assert_eq!(r[0]["title"], "季度计划");
    assert!(
        r.iter()
            .any(|x| x["page_id"] == b.as_str() && x["block_id"].is_string())
    );
    assert!(
        r[0]["snippet"]
            .as_str()
            .unwrap()
            .contains("<mark>计划</mark>")
    );
    assert!(r[0]["updated_at"].as_i64().unwrap() > 0);
    // 英文词干、前缀
    let r = search(&s, &c, "runs ", None).await;
    assert_eq!(r.len(), 1);
    assert!(
        r[0]["snippet"]
            .as_str()
            .unwrap()
            .contains("<mark>running</mark>"),
        "{r:?}"
    );
    assert_eq!(search(&s, &c, "engi", None).await[0]["page_id"], b.as_str());

    // 改一块：旧的词搜不到，新的搜得到
    let r = c
        .put(s.api(&format!("/pages/{a}/content")))
        .json(&json!({ "markdown": "我们下个季度要完成**搜索**功能。\n\n第二段改成讲招聘。" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 204);
    settle(&s, &[&a]).await;
    assert!(search(&s, &c, "预算", None).await.is_empty());
    assert_eq!(search(&s, &c, "招聘", None).await[0]["page_id"], a.as_str());
    // 改标题
    let (st, _) = json(
        c.patch(s.api(&format!("/pages/{a}")))
            .json(&json!({ "title": "年度目标" })),
    )
    .await;
    assert_eq!(st, 200);
    settle(&s, &[&a]).await;
    assert_eq!(search(&s, &c, "年度", None).await[0]["title"], "年度目标");

    // 数据库的行：属性能搜到，kind 过滤
    let (_, db) = json(c.post(s.api(&format!("/workspaces/{}/databases", s.ws_id()))).json(&json!({
        "title": "客户", "schema": { "props": [{ "name": "名称", "type": "title" }, { "name": "备注", "type": "text" }, { "name": "级别", "type": "select", "options": ["金牌"] }] }
    })))
    .await;
    let db = db["page"]["id"].as_str().unwrap().to_string();
    let (_, add) = json(c.post(s.api(&format!("/databases/{db}/rows"))).json(&json!({ "rows": [{ "props": { "名称": "华为", "备注": "重要的合作伙伴", "级别": "金牌" } }] }))).await;
    let row = add["ids"][0].as_str().unwrap().to_string();
    settle(&s, &[]).await;
    let r = search(&s, &c, "合作伙伴", None).await;
    assert_eq!(
        (
            r[0]["page_id"].as_str(),
            r[0]["kind"].as_str(),
            r[0]["db_id"].as_str()
        ),
        (Some(row.as_str()), Some("row"), Some(db.as_str())),
        "{r:?}"
    );
    assert_eq!(search(&s, &c, "金牌", Some("row")).await.len(), 1);
    assert!(search(&s, &c, "金牌", Some("page")).await.is_empty());
    // 改格子
    let (st, _) = json(
        c.patch(s.api(&format!("/databases/{db}/rows")))
            .json(&json!({ "ops": [{ "row_id": row, "set": { "备注": "老客户" } }] })),
    )
    .await;
    assert_eq!(st, 200);
    settle(&s, &[]).await;
    assert!(search(&s, &c, "合作伙伴", None).await.is_empty());
    assert_eq!(search(&s, &c, "老客户", None).await.len(), 1);

    // 删页：搜不到；删库：行也搜不到
    let (st, _) = json(c.delete(s.api(&format!("/pages/{b}")))).await;
    assert_eq!(st, 200);
    let (st, _) = json(c.delete(s.api(&format!("/pages/{db}")))).await;
    assert_eq!(st, 200);
    settle(&s, &[]).await;
    assert!(search(&s, &c, "engine", None).await.is_empty());
    assert!(search(&s, &c, "老客户", None).await.is_empty());

    // 别的工作区的搜不到；不是成员 403
    let other = s.core.storage.create_workspace("别的").await.unwrap();
    let (st, _) = json(c.get(s.api(&format!("/workspaces/{}/search?q=x", other.id)))).await;
    assert_eq!(st, 403);
    let (st, _) = json(c.get(s.api(&format!("/workspaces/{}/search?q=%20", s.ws_id())))).await;
    assert_eq!(st, 400);
    s.stop().await;
}

/// 重启：索引在盘上，不全量重建；停机期间改的页面启动时补上。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn catch_up_on_restart() {
    let dir = tempfile::tempdir().unwrap();
    let c = http();
    let ids: Vec<String> = {
        let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
        let mut ids = vec![];
        for i in 0..5 {
            ids.push(page_with(&s, &c, &format!("页面 {i}"), &format!("内容 编号{i} 苹果")).await);
        }
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        settle(&s, &refs).await;
        assert_eq!(search(&s, &c, "苹果", None).await.len(), 5);
        s.stop().await;
        ids
    };
    // 停机期间：另起一个不带搜索索引目录的实例改一页（模拟索引没跟上）
    {
        let s = Server::start_media(
            dir.path(),
            AuthConfig::None,
            fast_hub(),
            Default::default(),
            Default::default(),
            oad_core::MediaConfig {
                search_dir: Some(dir.path().join("其他索引")),
                ..Default::default()
            },
        )
        .await;
        let r = c
            .put(s.api(&format!("/pages/{}/content", ids[0])))
            .json(&json!({ "markdown": "香蕉" }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 204);
        s.stop().await;
    }
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    settle(&s, &[]).await;
    assert_eq!(
        s.core.search.processed.load(Ordering::Relaxed),
        1,
        "只补改过的那一页"
    );
    assert_eq!(search(&s, &c, "苹果", None).await.len(), 4);
    assert_eq!(
        search(&s, &c, "香蕉", None).await[0]["page_id"],
        ids[0].as_str()
    );
    s.stop().await;
}
