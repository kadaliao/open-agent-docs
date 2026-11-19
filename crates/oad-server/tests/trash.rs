//! 回收站：列出、恢复（连子页面、上级不在了放到最外层、行和库）、彻底删（正文、评论、索引、没人用的文件）、过期自动清。
mod common;
use common::*;
use oad_core::AuthConfig;
use serde_json::{Value, json};

async fn trash(s: &Server, c: &reqwest::Client) -> Vec<Value> {
    let (st, v) = json(c.get(s.api(&format!("/workspaces/{}/trash", s.ws_id())))).await;
    assert_eq!(st, 200, "{v}");
    v["items"].as_array().unwrap().clone()
}

async fn create(s: &Server, c: &reqwest::Client, title: &str, parent: Option<&str>) -> String {
    let (st, v) = json(
        c.post(s.api(&format!("/workspaces/{}/pages", s.ws_id())))
            .json(&json!({ "title": title, "parent_id": parent })),
    )
    .await;
    assert_eq!(st, 201, "{v}");
    v["id"].as_str().unwrap().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn restore_and_purge() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let a = create(&s, &c, "父页", None).await;
    let b = create(&s, &c, "子页", Some(&a)).await;
    let d = create(&s, &c, "孙页", Some(&b)).await;
    let x = create(&s, &c, "单独删的子页", Some(&a)).await;
    // 先单独删 x，再删 a（连 b、d）
    json(c.delete(s.api(&format!("/pages/{x}")))).await;
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let (st, _) = json(c.delete(s.api(&format!("/pages/{a}")))).await;
    assert_eq!(st, 200);
    let items = trash(&s, &c).await;
    let ids: Vec<&str> = items.iter().map(|i| i["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        vec![a.as_str(), x.as_str()],
        "跟着上级删的不单列，新删的在前"
    );
    assert_eq!(items[1]["parent_title"], "父页");
    assert_eq!(items[0]["deleted_by"]["kind"], "user");

    // 恢复 a：b、d 一起回来，x 不回来；发 page.created
    let sse = c
        .get(s.api(&format!("/workspaces/{}/events", s.ws_id())))
        .send()
        .await
        .unwrap();
    let (st, v) = json(c.post(s.api(&format!("/trash/{a}/restore")))).await;
    assert_eq!(st, 200, "{v}");
    assert_eq!(v["id"], a.as_str());
    assert_eq!(v["restored"].as_array().unwrap().len(), 3);
    let mut stream = sse.bytes_stream();
    let mut buf = String::new();
    while buf.matches("page.created").count() < 3 {
        use futures_util::StreamExt;
        let chunk = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        buf.push_str(&String::from_utf8_lossy(&chunk));
    }
    let (_, list) = json(c.get(s.api(&format!("/workspaces/{}/pages", s.ws_id())))).await;
    let live: Vec<&str> = list["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert!(
        live.contains(&b.as_str()) && live.contains(&d.as_str()) && !live.contains(&x.as_str())
    );

    // x 的上级在：放回原处；上级不在了：放到最外层
    let (st, v) = json(c.post(s.api(&format!("/trash/{x}/restore")))).await;
    assert_eq!(st, 200);
    assert_eq!(v["parent_id"], a.as_str());
    json(c.delete(s.api(&format!("/pages/{d}")))).await;
    json(c.delete(s.api(&format!("/pages/{b}")))).await;
    let (_, v) = json(c.post(s.api(&format!("/trash/{d}/restore")))).await;
    assert_eq!(v["parent_id"], Value::Null, "上级还在回收站里：放到最外层");
    let (st, _) = json(c.post(s.api(&format!("/trash/{a}/restore")))).await;
    assert_eq!(st, 400, "不在回收站里");

    // 彻底删：有评论、上传的图片；之后页面、评论、搜索、文件都没了
    let page = page_with(&s, &c, "要彻底删的", "一段独特的内容").await;
    let png = {
        let img = image::RgbImage::from_pixel(4, 4, image::Rgb([200, 10, 10]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    };
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(png).file_name("红.png"),
    );
    let (st, up) = json(
        c.post(s.api(&format!("/workspaces/{}/files", s.ws_id())))
            .multipart(form),
    )
    .await;
    assert_eq!(st, 201, "{up}");
    let url = up["url"].as_str().unwrap().to_string();
    let r = c
        .put(s.api(&format!("/pages/{page}/content")))
        .json(&json!({ "markdown": format!("一段独特的内容\n\n![]({url})") }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 204);
    let (st, _) = json(
        c.post(s.api(&format!("/pages/{page}/comments")))
            .json(&json!({ "body": "评论" })),
    )
    .await;
    assert_eq!(st, 201);
    let _ = s.core.hub.flush(&page).await;
    s.core.search.flush().await;
    let (_, r) = json(c.get(s.api(&format!("/workspaces/{}/search?q=独特", s.ws_id())))).await;
    assert_eq!(r["results"].as_array().unwrap().len(), 1);
    let (st, _) = json(c.delete(s.api(&format!("/trash/{page}")))).await;
    assert_eq!(st, 400, "要先进回收站");
    json(c.delete(s.api(&format!("/pages/{page}")))).await;
    let r = c
        .delete(s.api(&format!("/trash/{page}")))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 204);
    let (st, _) = json(c.get(s.api(&format!("/pages/{page}")))).await;
    assert_eq!(st, 404);
    assert!(trash(&s, &c).await.iter().all(|i| i["id"] != page.as_str()));
    s.core.search.flush().await;
    let (_, r) = json(c.get(s.api(&format!("/workspaces/{}/search?q=独特", s.ws_id())))).await;
    assert!(r["results"].as_array().unwrap().is_empty());
    let r = c
        .get(s.api(url.trim_start_matches("/api/v1")))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 404, "没有别的页面用的文件一起删了");
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rows_databases_and_expiry() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let (_, db) = json(
        c.post(s.api(&format!("/workspaces/{}/databases", s.ws_id())))
            .json(&json!({ "title": "库" })),
    )
    .await;
    let db = db["page"]["id"].as_str().unwrap().to_string();
    let (_, add) = json(
        c.post(s.api(&format!("/databases/{db}/rows")))
            .json(&json!({ "rows": [{ "props": {} }, { "props": {} }] })),
    )
    .await;
    let (r0, r1) = (
        add["ids"][0].as_str().unwrap().to_string(),
        add["ids"][1].as_str().unwrap().to_string(),
    );
    // 单独删的行在回收站里，恢复回到库里
    json(
        c.delete(s.api(&format!("/databases/{db}/rows")))
            .json(&json!({ "ids": [r0] })),
    )
    .await;
    let items = trash(&s, &c).await;
    assert_eq!(
        (
            items[0]["id"].as_str(),
            items[0]["kind"].as_str(),
            items[0]["parent_title"].as_str()
        ),
        (Some(r0.as_str()), Some("row"), Some("库"))
    );
    let (st, _) = json(c.post(s.api(&format!("/trash/{r0}/restore")))).await;
    assert_eq!(st, 200);
    let (_, rows) = json(c.get(s.api(&format!("/databases/{db}/rows")))).await;
    assert_eq!(rows["rows"].as_array().unwrap().len(), 2);
    // 删库：行跟着；库不在时行不能单独恢复
    json(
        c.delete(s.api(&format!("/databases/{db}/rows")))
            .json(&json!({ "ids": [r1] })),
    )
    .await;
    json(c.delete(s.api(&format!("/pages/{db}")))).await;
    let (st, _) = json(c.post(s.api(&format!("/trash/{r1}/restore")))).await;
    assert_eq!(st, 400);
    let (st, _) = json(c.post(s.api(&format!("/trash/{db}/restore")))).await;
    assert_eq!(st, 200);
    let (_, rows) = json(c.get(s.api(&format!("/databases/{db}/rows")))).await;
    assert_eq!(
        rows["rows"].as_array().unwrap().len(),
        1,
        "跟库一起删的行回来了，之前单独删的还在回收站"
    );

    // 过期自动清：放了 0 天以上的都清掉
    let p = page_with(&s, &c, "旧的", "x").await;
    json(c.delete(s.api(&format!("/pages/{p}")))).await;
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let n = s.core.purge_expired(0).await.unwrap();
    assert_eq!(n, 2, "旧页面和单独删的那一行");
    assert!(trash(&s, &c).await.is_empty());
    s.stop().await;
}
