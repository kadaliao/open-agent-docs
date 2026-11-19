//! 公开分享：建 / 改 / 关；服务器渲染的 HTML（标题、各种块、卡片、缩略图、公式和代码的脚本、noindex）；子页面的范围；
//! 分享里的文件；数据库表格和往下翻；行页面的属性；转义；首字节时间。
mod common;
use common::*;
use oad_core::AuthConfig;
use serde_json::{Value, json};
use std::time::Instant;

async fn get(s: &Server, path: &str) -> (u16, reqwest::header::HeaderMap, String) {
    let r = http()
        .get(format!("http://{}{path}", s.addr))
        .send()
        .await
        .unwrap();
    let st = r.status().as_u16();
    let h = r.headers().clone();
    (st, h, r.text().await.unwrap())
}

async fn share(s: &Server, c: &reqwest::Client, page: &str, body: Value) -> Value {
    let (st, v) = json(c.post(s.api(&format!("/pages/{page}/share"))).json(&body)).await;
    assert_eq!(st, 200, "{v}");
    v
}

fn png() -> Vec<u8> {
    let img = image::RgbImage::from_pixel(1600, 900, image::Rgb([20, 120, 200]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn render_and_scope() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let ws = s.ws_id();
    // 上传一张图，放进页面
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(png()).file_name("图.png"),
    );
    let (_, up) = json(
        c.post(s.api(&format!("/workspaces/{ws}/files")))
            .multipart(form),
    )
    .await;
    let img_url = up["url"].as_str().unwrap().to_string();
    // 一个视频卡片的解析缓存
    let yt = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
    s.core
        .storage
        .put_unfurl(oad_core::UnfurlRow {
            url_key: oad_unfurl::cache_key(yt),
            data: Some(json!({
                "kind": "video", "provider": "youtube", "url": yt, "title": "一个视频",
                "thumbnail": { "url": "https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg" },
                "embed": { "type": "youtube", "embed_url": "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ" }
            })),
            ok: true,
            fetched_at: oad_core::ids::now_ms(),
            expires_at: oad_core::ids::now_ms() + 3_600_000,
            error: None,
            failures: 0,
        })
        .await
        .unwrap();
    let page = page_with(&s, &c, "公开<页面>", "x").await;
    let (_, child) = json(
        c.post(s.api(&format!("/workspaces/{ws}/pages")))
            .json(&json!({ "title": "子页面", "parent_id": page })),
    )
    .await;
    let child = child["id"].as_str().unwrap().to_string();
    let other = page_with(&s, &c, "没分享的页面", "秘密").await;
    let md = format!(
        "## 小标题\n\n**粗体**和[链接](https://example.com)，[坏链接](javascript:alert(1))，[子页面](/p/{child})，[别的](/p/{other})。\n\n\
         - 一\n- 二\n\n- [x] 做完了\n\n```rust\nfn main() {{}}\n```\n\n$$\nE=mc^2\n$$\n\n![示意图]({img_url})\n\n{yt}\n\n> 引用 <script>alert(1)</script>\n\n| a | b |\n|---|---|\n| 1 | 2 |"
    );
    let r = c
        .put(s.api(&format!("/pages/{page}/content")))
        .json(&json!({ "markdown": md }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 204);
    let _ = s.core.hub.flush(&page).await;
    s.core.search.flush().await; // 记下页面用了哪些文件

    // 没分享：404；分享
    let (st, _) = json(c.get(s.api(&format!("/pages/{page}/share")))).await;
    assert_eq!(st, 404);
    let sh = share(&s, &c, &page, json!({})).await;
    let token = sh["token"].as_str().unwrap().to_string();
    assert!(
        sh["url"]
            .as_str()
            .unwrap()
            .ends_with(&format!("/s/{token}"))
    );
    assert_eq!(sh["include_children"], false);
    let (_, again) = json(c.get(s.api(&format!("/pages/{page}/share")))).await;
    assert_eq!(again["token"], token.as_str());

    // 不用登录就能看
    let t = Instant::now();
    let (st, h, html) = get(&s, &format!("/s/{token}")).await;
    let ttfb = t.elapsed();
    assert_eq!(st, 200, "{html}");
    assert!(h["content-type"].to_str().unwrap().starts_with("text/html"));
    assert_eq!(h["x-robots-tag"], "noindex, nofollow");
    assert!(html.contains("<meta name=\"robots\" content=\"noindex, nofollow\">"));
    assert!(html.contains("公开&lt;页面&gt;"), "标题转义");
    assert!(
        html.contains("<h3") && html.contains("<strong>粗体</strong>"),
        "页面标题是 h1，正文的标题往下一级"
    );
    assert!(html.contains("href=\"https://example.com\""));
    assert!(!html.contains("javascript:"), "危险的链接不当链接");
    assert!(
        html.contains("&lt;script&gt;alert(1)&lt;/script&gt;") && !html.contains("<script>alert")
    );
    assert!(html.contains("class=\"language-rust\"") && html.contains("data-latex=\"E=mc^2\""));
    assert!(html.contains("input type=\"checkbox\" disabled checked"));
    assert!(html.contains("<table>"));
    assert!(
        html.contains("data-embed=\"https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ\""),
        "视频门面"
    );
    assert!(
        html.contains("window.OAD_NEEDS=")
            && html.contains("\"math\"")
            && html.contains("\"code\"")
    );
    let csp = h["content-security-policy"].to_str().unwrap();
    let nonce = csp
        .split("'nonce-")
        .nth(1)
        .unwrap()
        .split('\'')
        .next()
        .unwrap();
    assert!(html.contains(&format!("<script nonce=\"{nonce}\">")));
    // 没带子页面：子页面的链接不链，也打不开
    assert!(!html.contains(&format!("/s/{token}/{child}")));
    assert!(!html.contains(&format!("/p/{other}")) && !html.contains("秘密"));
    let (st, _, _) = get(&s, &format!("/s/{token}/{child}")).await;
    assert_eq!(st, 404);
    // 图片：缩略图走分享下的地址
    let file_id = img_url.split('/').nth(4).unwrap();
    let src = format!("/s/{token}/_/files/{file_id}/");
    assert!(
        html.contains(&src) && html.contains("?w=640 640w"),
        "{html}"
    );
    let img = html.split(&src).nth(1).unwrap().split('"').next().unwrap();
    let (st, h, _) = get(&s, &format!("{src}{img}")).await;
    assert_eq!(st, 200);
    assert_eq!(h["content-type"], "image/webp", "缩略图");
    let up2 = {
        let form = reqwest::multipart::Form::new().part(
            "file",
            reqwest::multipart::Part::bytes(png()).file_name("别的.png"),
        );
        json(
            c.post(s.api(&format!("/workspaces/{ws}/files")))
                .multipart(form),
        )
        .await
        .1
    };
    let other_file = up2["id"].as_str().unwrap();
    let (st, _, _) = get(&s, &format!("/s/{token}/_/files/{other_file}/x.png")).await;
    assert_eq!(st, 404, "页面没用的文件不给");
    println!("分享页首字节：{ttfb:?}");

    // 带子页面、允许搜索引擎：链接不变
    let sh2 = share(
        &s,
        &c,
        &page,
        json!({ "include_children": true, "indexable": true }),
    )
    .await;
    assert_eq!(sh2["token"], token.as_str());
    let (_, h, html) = get(&s, &format!("/s/{token}")).await;
    assert!(h.get("x-robots-tag").is_none() && !html.contains("noindex"));
    assert!(
        html.contains(&format!("href=\"/s/{token}/{child}\"")),
        "子页面列表和链接"
    );
    let (st, _, chtml) = get(&s, &format!("/s/{token}/{child}")).await;
    assert_eq!(st, 200);
    assert!(chtml.contains("class=\"crumbs\"") && chtml.contains(&format!("href=\"/s/{token}\"")));
    let (st, _, _) = get(&s, &format!("/s/{token}/{other}")).await;
    assert_eq!(st, 404, "不在分享范围里");

    // 关掉
    let r = c
        .delete(s.api(&format!("/pages/{page}/share")))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 204);
    let (st, h, _) = get(&s, &format!("/s/{token}")).await;
    assert_eq!(st, 404);
    assert!(h["x-robots-tag"].to_str().unwrap().contains("noindex"));
    let (st, _, _) = get(&s, &format!("{src}{img}")).await;
    assert_eq!(st, 404);
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn database_share() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let (_, db) = json(c.post(s.api(&format!("/workspaces/{}/databases", s.ws_id()))).json(&json!({
        "title": "公开的库", "schema": { "props": [{ "name": "名称", "type": "title" }, { "name": "价格", "type": "number" }, { "name": "状态", "type": "status" }] }
    })))
    .await;
    let db = db["page"]["id"].as_str().unwrap().to_string();
    let rows: Vec<Value> = (0..250)
        .map(|i| json!({ "props": { "名称": format!("第{i}行"), "价格": i, "状态": "进行中" } }))
        .collect();
    let (_, add) = json(
        c.post(s.api(&format!("/databases/{db}/rows")))
            .json(&json!({ "rows": rows })),
    )
    .await;
    let first = add["ids"][0].as_str().unwrap().to_string();
    let token = share(&s, &c, &db, json!({})).await["token"]
        .as_str()
        .unwrap()
        .to_string();
    let (st, _, html) = get(&s, &format!("/s/{token}")).await;
    assert_eq!(st, 200);
    assert_eq!(html.matches("<tr>").count(), 201, "表头 + 前 200 行");
    assert!(
        html.contains("<th>名称</th>")
            && html.contains("<td>进行中</td>")
            && html.contains("250 行")
    );
    let more = html
        .split("data-more=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .replace("&amp;", "&");
    let (st, _, m) = get(&s, &more).await;
    assert_eq!(st, 200);
    let m: Value = serde_json::from_str(&m).unwrap();
    assert_eq!(m["html"].as_str().unwrap().matches("<tr>").count(), 50);
    assert!(m["next"].is_null());
    // 行页面：先列属性
    let (st, _, row) = get(&s, &format!("/s/{token}/{first}")).await;
    assert_eq!(st, 200);
    assert!(row.contains("<dt>价格</dt><dd>0</dd>") && row.contains("第0行"));

    // 嵌在页面里的库：在分享范围里才画表格
    let page = page_with(&s, &c, "带库的页面", &format!("前面\n\n/p/{db}")).await;
    let _ = page;
    let r = c
        .put(s.api(&format!("/pages/{page}/content")))
        .json(&json!({ "markdown": format!("前面\n\n<http://localhost/p/{db}>") }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 204);
    let t2 = share(&s, &c, &page, json!({})).await["token"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, _, h) = get(&s, &format!("/s/{t2}")).await;
    assert!(!h.contains("<th>名称</th>"), "库不在分享范围里");
    s.stop().await;
}

/// 首字节时间：1000 块的页面。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn share_ttfb() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    for n in [100, 1000, 10000] {
        let md: String = (0..n)
            .map(|i| format!("第 {i} 段，有 **加粗** 和 [链接](https://a.com/{i})。\n\n"))
            .collect();
        let page = page_with(&s, &c, &format!("{n} 段"), &md).await;
        let token = share(&s, &c, &page, json!({})).await["token"]
            .as_str()
            .unwrap()
            .to_string();
        get(&s, &format!("/s/{token}")).await;
        let mut v = vec![];
        for _ in 0..20 {
            let t = Instant::now();
            let r = http()
                .get(format!("http://{}/s/{token}", s.addr))
                .send()
                .await
                .unwrap();
            v.push(t.elapsed());
            let _ = r.bytes().await;
        }
        v.sort();
        println!("分享页 {n} 块：首字节 p50 {:?}，p95 {:?}", v[10], v[18]);
    }
    s.stop().await;
}
