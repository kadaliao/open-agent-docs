//! 第 4 阶段：数据库的接口（建库、行、每格最后写入为准、表结构和撤销、视图、查询、按钮、CSV、实时事件、嵌入、MCP）。
mod common;
use common::*;
use futures_util::StreamExt;
use oad_core::AuthConfig;
use serde_json::{Value, json};
use std::io::Read;
use std::time::Duration;

/// 读 SSE 里名字以 `prefix` 开头的事件，凑够 n 个。
async fn read_events(
    res: reqwest::Response,
    prefix: &'static str,
    n: usize,
) -> Vec<(String, Value)> {
    let mut stream = res.bytes_stream();
    let mut buf = String::new();
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while out.len() < n {
        let chunk = tokio::time::timeout_at(deadline, stream.next())
            .await
            .expect("等事件超时")
            .unwrap()
            .unwrap();
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(i) = buf.find("\n\n") {
            let block: String = buf.drain(..i + 2).collect();
            let mut ev = None;
            let mut data = String::new();
            for line in block.lines() {
                if let Some(e) = line.strip_prefix("event:") {
                    ev = Some(e.trim().to_string());
                } else if let Some(d) = line.strip_prefix("data:") {
                    data.push_str(d.trim());
                }
            }
            if let Some(ev) = ev.filter(|e| e.starts_with(prefix)) {
                out.push((ev, serde_json::from_str(&data).unwrap()));
            }
        }
    }
    out
}

struct Db {
    id: String,
    schema: Value,
}

impl Db {
    /// 属性 key（按名字）。
    fn key(&self, name: &str) -> String {
        self.schema["props"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .unwrap_or_else(|| panic!("没有属性 {name}：{}", self.schema))["key"]
            .as_str()
            .unwrap()
            .to_string()
    }
    fn opt(&self, prop: &str, name: &str) -> String {
        let k = self.key(prop);
        self.schema["props"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["key"] == k.as_str())
            .unwrap()["options"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["name"] == name)
            .unwrap_or_else(|| panic!("「{prop}」没有选项 {name}"))["id"]
            .as_str()
            .unwrap()
            .to_string()
    }
}

async fn create_db(s: &Server, c: &reqwest::Client, title: &str, schema: Value) -> Db {
    let (st, v) = json(
        c.post(s.api(&format!("/workspaces/{}/databases", s.ws_id())))
            .json(&json!({ "title": title, "schema": schema })),
    )
    .await;
    assert_eq!(st, 201, "{v}");
    assert_eq!(v["page"]["kind"], "database");
    Db {
        id: v["page"]["id"].as_str().unwrap().into(),
        schema: v["schema"].clone(),
    }
}

async fn refresh(s: &Server, c: &reqwest::Client, db: &mut Db) -> Value {
    let (st, v) = json(c.get(s.api(&format!("/databases/{}", db.id)))).await;
    assert_eq!(st, 200, "{v}");
    db.schema = v["schema"].clone();
    v
}

async fn add(s: &Server, c: &reqwest::Client, db: &str, rows: Value) -> (Vec<String>, i64) {
    let (st, v) = json(
        c.post(s.api(&format!("/databases/{db}/rows")))
            .json(&json!({ "rows": rows })),
    )
    .await;
    assert_eq!(st, 201, "{v}");
    let ids = v["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect();
    (ids, v["seq"].as_i64().unwrap())
}

async fn rows(s: &Server, c: &reqwest::Client, db: &str, since: Option<i64>) -> Value {
    let url = match since {
        Some(x) => s.api(&format!("/databases/{db}/rows?since={x}")),
        None => s.api(&format!("/databases/{db}/rows")),
    };
    let (st, v) = json(c.get(url)).await;
    assert_eq!(st, 200, "{v}");
    v
}

fn row<'a>(rows: &'a Value, id: &str) -> &'a Value {
    rows["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap_or_else(|| panic!("没有行 {id}"))
}

async fn patch(s: &Server, c: &reqwest::Client, db: &str, ops: Value) -> (u16, Value) {
    json(
        c.patch(s.api(&format!("/databases/{db}/rows")))
            .json(&json!({ "ops": ops })),
    )
    .await
}

async fn schema_ops(s: &Server, c: &reqwest::Client, db: &str, ops: Value) -> (u16, Value) {
    json(
        c.patch(s.api(&format!("/databases/{db}/schema")))
            .json(&json!({ "ops": ops })),
    )
    .await
}

fn task_schema() -> Value {
    json!({ "props": [
        { "name": "任务", "type": "title" },
        { "name": "状态", "type": "status" },
        { "name": "价格", "type": "number" },
        { "name": "截止", "type": "date" },
        { "name": "完成", "type": "checkbox" },
        { "name": "标签", "type": "multi_select", "options": [{ "name": "急" }] },
        { "name": "编号", "type": "unique_id", "prefix": "T-" },
    ]})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rows_cells_and_sync() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let ws = s.ws_id();
    let sse = c
        .get(s.api(&format!("/workspaces/{ws}/events")))
        .send()
        .await
        .unwrap();
    let events = tokio::spawn(read_events(sse, "db.", 2));
    let mut db = create_db(&s, &c, "任务表", task_schema()).await;
    assert_eq!(db.schema["props"][0]["type"], "title");
    assert_eq!(db.schema["props"].as_array().unwrap().len(), 7);
    // 状态有默认的三组选项
    db.opt("状态", "进行中");

    // 加行：写名字（选项名、日期、勾选的“是”、新标签自动加选项），也能写存的样子
    let (ids, seq1) = add(
        &s,
        &c,
        &db.id,
        json!([
            { "props": { "任务": "写周报", "状态": "进行中", "价格": "1,200", "截止": "2026-11-20", "完成": "是", "标签": "急、新的" }, "body": "第一段\n\n- 要点" },
            { "props": { "任务": "买菜" } },
        ]),
    )
    .await;
    assert_eq!(ids.len(), 2);
    let evs = events.await.unwrap();
    assert!(
        evs.iter()
            .any(|(n, d)| n == "db.changed" && d["db_id"] == db.id.as_str() && d["seq"] == seq1)
    );
    assert!(
        evs.iter()
            .any(|(n, d)| n == "db.schema" && d["db_id"] == db.id.as_str()),
        "新加了选项：{evs:?}"
    );
    refresh(&s, &c, &mut db).await;
    let all = rows(&s, &c, &db.id, None).await;
    assert_eq!(all["seq"], seq1);
    let r0 = row(&all, &ids[0]);
    assert_eq!(r0["props"][db.key("任务")], "写周报");
    assert_eq!(
        r0["props"][db.key("状态")],
        db.opt("状态", "进行中").as_str()
    );
    assert_eq!(r0["props"][db.key("价格")], 1200);
    assert_eq!(r0["props"][db.key("截止")]["start"], "2026-11-20");
    assert_eq!(r0["props"][db.key("完成")], true);
    assert_eq!(
        r0["props"][db.key("标签")],
        json!([db.opt("标签", "急"), db.opt("标签", "新的")])
    );
    assert_eq!(r0["unique_id"], 1);
    assert_eq!(row(&all, &ids[1])["unique_id"], 2);
    assert!(r0["created_by"].as_str().unwrap().starts_with("user:"));
    // 顺序：按库里的位置
    assert_eq!(all["rows"][0]["id"], ids[0].as_str());

    // 行是页面：有标题、正文；侧栏不列
    let (st, p) = json(c.get(s.api(&format!("/pages/{}", ids[0])))).await;
    assert_eq!(st, 200);
    assert_eq!(
        (
            p["kind"].as_str(),
            p["title"].as_str(),
            p["parent_id"].as_str()
        ),
        (Some("row"), Some("写周报"), Some(db.id.as_str()))
    );
    let (_, content) =
        json(c.get(s.api(&format!("/pages/{}/content?format=markdown", ids[0])))).await;
    assert!(content.to_string().contains("要点"), "{content}");
    let (_, list) = json(c.get(s.api(&format!("/workspaces/{ws}/pages")))).await;
    let kinds: Vec<&str> = list["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, vec!["database"]);

    // 插在第一行后面
    let (mid, _) = add(
        &s,
        &c,
        &db.id,
        json!([{ "props": { "任务": "中间" }, "after": ids[0] }]),
    )
    .await;
    let all = rows(&s, &c, &db.id, None).await;
    let order: Vec<&str> = all["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        order,
        vec![ids[0].as_str(), mid[0].as_str(), ids[1].as_str()]
    );

    // 两个人同时改同一行的不同格子：都留住；同一格后写的为准
    let (k_price, k_state) = (db.key("价格"), db.key("状态"));
    let a = patch(
        &s,
        &c,
        &db.id,
        json!([{ "row_id": ids[1], "set": { k_price.clone(): 5 } }]),
    );
    let b = patch(
        &s,
        &c,
        &db.id,
        json!([{ "row_id": ids[1], "set": { "状态": "已完成" } }]),
    );
    let ((sa, va), (sb, vb)) = tokio::join!(a, b);
    assert_eq!((sa, sb), (200, 200), "{va} {vb}");
    let (st, _) = patch(
        &s,
        &c,
        &db.id,
        json!([{ "row_id": ids[1], "set": { "价格": 6 } }]),
    )
    .await;
    assert_eq!(st, 200);
    let (_, last) = patch(
        &s,
        &c,
        &db.id,
        json!([{ "row_id": ids[1], "set": { "价格": 7 } }]),
    )
    .await;
    let all = rows(&s, &c, &db.id, None).await;
    let r1 = row(&all, &ids[1]);
    assert_eq!(r1["props"][&k_price], 7);
    assert_eq!(r1["props"][&k_state], db.opt("状态", "已完成").as_str());
    assert_eq!(all["seq"], last["seq"]);
    // 值和原来一样：没有改动，seq 不涨
    let (_, same) = patch(
        &s,
        &c,
        &db.id,
        json!([{ "row_id": ids[1], "set": { "价格": 7 } }]),
    )
    .await;
    assert_eq!(same["seq"], last["seq"]);
    assert_eq!(same["changes"], json!([]));

    // 增量：只给 since 之后改过的
    let since = last["seq"].as_i64().unwrap();
    let (_, p2) = patch(
        &s,
        &c,
        &db.id,
        json!([{ "row_id": ids[0], "set": { "价格": null } }]),
    )
    .await;
    let d = rows(&s, &c, &db.id, Some(since)).await;
    assert_eq!(d["seq"], p2["seq"]);
    assert_eq!(d["rows"].as_array().unwrap().len(), 1);
    assert!(d["rows"][0]["props"].get(&k_price).is_none(), "null 清空");
    // 删行进回收站，增量里在 deleted
    let (st, _) = json(
        c.delete(s.api(&format!("/databases/{}/rows", db.id)))
            .json(&json!({ "ids": [mid[0]] })),
    )
    .await;
    assert_eq!(st, 200);
    let d = rows(&s, &c, &db.id, Some(p2["seq"].as_i64().unwrap())).await;
    assert_eq!(d["deleted"], json!([mid[0]]));
    assert_eq!(
        rows(&s, &c, &db.id, None).await["rows"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let (st, _) = json(c.get(s.api(&format!("/pages/{}", mid[0])))).await;
    assert_eq!(st, 404);

    // 错误：只读的属性、没有的属性、不认识的值
    let (st, v) = patch(
        &s,
        &c,
        &db.id,
        json!([{ "row_id": ids[0], "set": { "编号": 9 } }]),
    )
    .await;
    assert_eq!(st, 400, "{v}");
    let (st, v) = patch(
        &s,
        &c,
        &db.id,
        json!([{ "row_id": ids[0], "set": { "没有这列": 1 } }]),
    )
    .await;
    assert_eq!(st, 400);
    assert!(
        v["error"]["message"].as_str().unwrap().contains("价格"),
        "列出现有的属性：{v}"
    );
    let (st, _) = patch(
        &s,
        &c,
        &db.id,
        json!([{ "row_id": ids[0], "set": { "价格": "很贵" } }]),
    )
    .await;
    assert_eq!(st, 400);
    let (st, _) = patch(
        &s,
        &c,
        &db.id,
        json!([{ "row_id": "nope", "set": { "价格": 1 } }]),
    )
    .await;
    assert_eq!(st, 404);
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn title_is_row_page_title() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let db = create_db(
        &s,
        &c,
        "库",
        json!({ "props": [{ "name": "名称", "type": "title" }] }),
    )
    .await;
    let (ids, _) = add(&s, &c, &db.id, json!([{ "props": { "名称": "旧" } }])).await;
    let id = &ids[0];
    // 改标题那一格 → 行页面的标题（正文文档里的标题也是）
    let (st, _) = patch(
        &s,
        &c,
        &db.id,
        json!([{ "row_id": id, "set": { "名称": "新" } }]),
    )
    .await;
    assert_eq!(st, 200);
    let (_, p) = json(c.get(s.api(&format!("/pages/{id}")))).await;
    assert_eq!(p["title"], "新");
    // 改行页面的标题（像网页里在行页面上改）→ 标题那一格跟着改，发 db.changed
    let sse = c
        .get(s.api(&format!("/workspaces/{}/events", s.ws_id())))
        .send()
        .await
        .unwrap();
    let events = tokio::spawn(read_events(sse, "db.changed", 1));
    let (st, _) = json(
        c.patch(s.api(&format!("/pages/{id}")))
            .json(&json!({ "title": "页面上改的" })),
    )
    .await;
    assert_eq!(st, 200);
    let ev = events.await.unwrap();
    assert_eq!(ev[0].1["db_id"], db.id.as_str());
    let all = rows(&s, &c, &db.id, None).await;
    assert_eq!(row(&all, id)["props"][db.key("名称")], "页面上改的");
    // 在 /pages 上删行：库里也删掉
    let (st, _) = json(c.delete(s.api(&format!("/pages/{id}")))).await;
    assert_eq!(st, 200);
    let all = rows(&s, &c, &db.id, Some(0)).await;
    assert_eq!(all["deleted"], json!([id]));
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn schema_changes_and_undo() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let mut db = create_db(&s, &c, "库", json!({ "props": [{ "name": "名称", "type": "title" }, { "name": "备注", "type": "text" }] })).await;
    let (ids, _) = add(
        &s,
        &c,
        &db.id,
        json!([{ "props": { "名称": "甲", "备注": "12" } }, { "props": { "名称": "乙", "备注": "不是数" } }]),
    )
    .await;

    // 加公式、加单选（带选项）
    let (st, v) = schema_ops(
        &s,
        &c,
        &db.id,
        json!([
            { "op": "add", "name": "分数", "type": "number" },
            { "op": "add", "name": "翻倍", "type": "formula", "formula": "prop(\"分数\") * 2" },
            { "op": "add", "name": "颜色", "type": "select", "options": ["红", "绿"] },
        ]),
    )
    .await;
    assert_eq!(st, 200, "{v}");
    assert_eq!(v["changes"].as_array().unwrap().len(), 3);
    refresh(&s, &c, &mut db).await;
    let (_, _) = patch(
        &s,
        &c,
        &db.id,
        json!([{ "row_id": ids[0], "set": { "分数": 21, "颜色": "红" } }]),
    )
    .await;
    let (_, q) = json(
        c.post(s.api(&format!("/databases/{}/query", db.id)))
            .json(&json!({})),
    )
    .await;
    let r = q["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == ids[0].as_str())
        .unwrap();
    assert_eq!(r["computed"][db.key("翻倍")], 42);

    // 公式写错：整批不改，说清楚哪里错
    let (st, v) = schema_ops(
        &s,
        &c,
        &db.id,
        json!([{ "op": "add", "name": "坏", "type": "formula", "formula": "prop(\"没有\") +" }]),
    )
    .await;
    assert_eq!(st, 400, "{v}");
    let (st, _) = schema_ops(
        &s,
        &c,
        &db.id,
        json!([{ "op": "set_formula", "prop": "翻倍", "formula": "prop(\"翻倍\") + 1" }]),
    )
    .await;
    assert_eq!(st, 400, "用到自己");
    let (st, _) = schema_ops(&s, &c, &db.id, json!([{ "op": "delete", "prop": "名称" }])).await;
    assert_eq!(st, 400, "标题不能删");
    let (st, _) = schema_ops(
        &s,
        &c,
        &db.id,
        json!([{ "op": "add", "name": "备注", "type": "text" }]),
    )
    .await;
    assert_eq!(st, 400, "重名");

    // 改名：公式里的名字跟着改
    let (st, v) = schema_ops(
        &s,
        &c,
        &db.id,
        json!([{ "op": "rename", "prop": "分数", "name": "得分" }]),
    )
    .await;
    assert_eq!(st, 200, "{v}");
    refresh(&s, &c, &mut db).await;
    let f = db.schema["props"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "翻倍")
        .unwrap()["formula"]
        .clone();
    assert_eq!(f, "prop(\"得分\") * 2");

    // 改类型：文字 → 数字，能转的转，转不了的清空；撤销放回原来的值
    let k_note = db.key("备注");
    let (st, v) = schema_ops(
        &s,
        &c,
        &db.id,
        json!([{ "op": "change_type", "prop": "备注", "type": "number" }]),
    )
    .await;
    assert_eq!(st, 200, "{v}");
    let conv_seq = v["seq"].as_i64().unwrap();
    let all = rows(&s, &c, &db.id, None).await;
    assert_eq!(row(&all, &ids[0])["props"][&k_note], 12);
    assert!(row(&all, &ids[1])["props"].get(&k_note).is_none());

    // 删列，再撤销：列和值都回来
    let k_color = db.key("颜色");
    let (st, v) = schema_ops(&s, &c, &db.id, json!([{ "op": "delete", "prop": "颜色" }])).await;
    assert_eq!(st, 200, "{v}");
    let del_seq = v["seq"].as_i64().unwrap();
    assert!(
        row(&rows(&s, &c, &db.id, None).await, &ids[0])["props"]
            .get(&k_color)
            .is_none()
    );
    let (st, v) = json(
        c.post(s.api(&format!("/databases/{}/schema/undo", db.id)))
            .json(&json!({ "seq": del_seq })),
    )
    .await;
    assert_eq!(st, 200, "{v}");
    refresh(&s, &c, &mut db).await;
    assert_eq!(db.key("颜色"), k_color);
    let all = rows(&s, &c, &db.id, None).await;
    assert_eq!(
        row(&all, &ids[0])["props"][&k_color],
        db.opt("颜色", "红").as_str()
    );

    // 撤销更早的改类型：类型回到文字、值放回去；之后别人改过的格子不动
    let (_, _) = patch(
        &s,
        &c,
        &db.id,
        json!([{ "row_id": ids[0], "set": { "备注": 99 } }]),
    )
    .await;
    let (st, v) = json(
        c.post(s.api(&format!("/databases/{}/schema/undo", db.id)))
            .json(&json!({ "seq": conv_seq })),
    )
    .await;
    assert_eq!(st, 200, "{v}");
    refresh(&s, &c, &mut db).await;
    let note = db.schema["props"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["key"] == k_note.as_str())
        .unwrap()
        .clone();
    assert_eq!(note["type"], "text");
    let all = rows(&s, &c, &db.id, None).await;
    assert_eq!(
        row(&all, &ids[0])["props"][&k_note],
        99,
        "撤销后被改过的格子留着"
    );
    assert_eq!(row(&all, &ids[1])["props"][&k_note], "不是数");
    let (st, _) = json(
        c.post(s.api(&format!("/databases/{}/schema/undo", db.id)))
            .json(&json!({ "seq": 9999 })),
    )
    .await;
    assert_eq!(st, 404);

    // 删掉选项：用了它的格子清掉
    let (st, v) = schema_ops(
        &s,
        &c,
        &db.id,
        json!([{ "op": "set_options", "prop": "颜色", "options": ["绿"] }]),
    )
    .await;
    assert_eq!(st, 200, "{v}");
    assert!(
        row(&rows(&s, &c, &db.id, None).await, &ids[0])["props"]
            .get(&k_color)
            .is_none()
    );

    // 挪位置
    let (st, v) = schema_ops(&s, &c, &db.id, json!([{ "op": "move", "prop": "颜色" }])).await;
    assert_eq!(st, 200, "{v}");
    assert_eq!(v["schema"]["props"][1]["name"], "颜色");
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn views_query_button() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let mut db = create_db(&s, &c, "任务表", task_schema()).await;
    let mut want = vec![];
    for i in 0..30 {
        let st = ["未开始", "进行中", "已完成"][i % 3];
        want.push(json!({ "props": { "任务": format!("任务 {i}"), "状态": st, "价格": i } }));
    }
    add(&s, &c, &db.id, Value::Array(want)).await;
    let info = refresh(&s, &c, &mut db).await;
    let views = info["views"].as_array().unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0]["type"], "table");

    // 视图：加、改、删
    let (st, v) = json(
        c.post(s.api(&format!("/databases/{}/views", db.id))).json(&json!({
            "name": "看板", "type": "board", "group_by": db.key("状态"),
            "filter": { "op": "and", "items": [{ "prop": db.key("价格"), "cond": "gte", "value": 10 }] },
            "sorts": [{ "prop": db.key("价格"), "dir": "desc" }], "hidden": [db.key("截止"), "没有这个"]
        })),
    )
    .await;
    assert_eq!(st, 201, "{v}");
    let board = v["id"].as_str().unwrap().to_string();
    assert_eq!(v["hidden"], json!([db.key("截止")]), "指向没有的属性的去掉");
    let (st, v) = json(
        c.patch(s.api(&format!("/databases/{}/views/{board}", db.id)))
            .json(&json!({ "name": "按状态" })),
    )
    .await;
    assert_eq!(st, 200, "{v}");
    assert_eq!(v["name"], "按状态");
    assert_eq!(v["type"], "board");
    let (st, _) = json(
        c.post(s.api(&format!("/databases/{}/views", db.id)))
            .json(&json!({ "type": "pie" })),
    )
    .await;
    assert_eq!(st, 400);

    // 按视图查：筛选、排序、分组
    let (st, q) = json(
        c.post(s.api(&format!("/databases/{}/query", db.id)))
            .json(&json!({ "view_id": board, "limit": 5 })),
    )
    .await;
    assert_eq!(st, 200, "{q}");
    assert_eq!(q["total"], 20);
    assert_eq!(q["ids"].as_array().unwrap().len(), 5);
    assert_eq!(q["rows"][0]["props"][db.key("价格")], 29);
    let groups = q["groups"].as_array().unwrap();
    assert_eq!(
        groups
            .iter()
            .map(|g| g["count"].as_u64().unwrap())
            .sum::<u64>(),
        20
    );
    // 直接给条件（写名字）、底部汇总
    let (st, q) = json(c.post(s.api(&format!("/databases/{}/query", db.id))).json(&json!({
        "filter": { "op": "and", "items": [{ "prop": "状态", "cond": "eq", "value": "进行中" }] },
        "totals": { db.key("价格"): "sum" }
    })))
    .await;
    assert_eq!(st, 200, "{q}");
    assert_eq!(q["total"], 10);
    assert_eq!(
        q["totals"][db.key("价格")],
        (0..30).filter(|i| i % 3 == 1).sum::<i64>()
    );
    let (st, _) = json(c.post(s.api(&format!("/databases/{}/query", db.id))).json(&json!({
        "filter": { "op": "and", "items": [{ "prop": "状态", "cond": "eq", "value": "没有的状态" }] }
    })))
    .await;
    assert_eq!(st, 400);

    // 删视图；最后一个删不了
    let (st, _) = json(c.delete(s.api(&format!("/databases/{}/views/{board}", db.id)))).await;
    assert_eq!(st, 204);
    let first = views[0]["id"].as_str().unwrap();
    let (st, _) = json(c.delete(s.api(&format!("/databases/{}/views/{first}", db.id)))).await;
    assert_eq!(st, 400);
    // 整个换掉
    let (st, v) = json(
        c.put(s.api(&format!("/databases/{}/views", db.id)))
            .json(&json!([{ "name": "日历", "type": "calendar", "calendar_by": db.key("截止") }])),
    )
    .await;
    assert_eq!(st, 200, "{v}");
    assert_eq!(v["views"][0]["type"], "calendar");

    // 按钮：点了改状态、发评论
    let (st, v) = schema_ops(
        &s,
        &c,
        &db.id,
        json!([{ "op": "add", "name": "做完", "type": "button", "label": "完成", "set": { "状态": "已完成", "完成": true }, "comment": "做完了" }]),
    )
    .await;
    assert_eq!(st, 200, "{v}");
    refresh(&s, &c, &mut db).await;
    let target = rows(&s, &c, &db.id, None).await["rows"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (st, v) = json(c.post(s.api(&format!(
        "/databases/{}/button/{target}/{}",
        db.id,
        db.key("做完")
    ))))
    .await;
    assert_eq!(st, 200, "{v}");
    assert!(v["thread_id"].is_string());
    let all = rows(&s, &c, &db.id, None).await;
    assert_eq!(
        row(&all, &target)["props"][db.key("状态")],
        db.opt("状态", "已完成").as_str()
    );
    assert_eq!(row(&all, &target)["props"][db.key("完成")], true);
    let (_, th) = json(c.get(s.api(&format!("/pages/{target}/comments")))).await;
    assert!(th.to_string().contains("做完了"), "{th}");
    let (st, _) = json(c.post(s.api(&format!(
        "/databases/{}/button/{target}/{}",
        db.id,
        db.key("价格")
    ))))
    .await;
    assert_eq!(st, 404, "不是按钮");
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn relations_and_rollups() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let mut people = create_db(
        &s,
        &c,
        "项目",
        json!({ "props": [{ "name": "名称", "type": "title" }] }),
    )
    .await;
    let mut tasks = create_db(&s, &c, "任务", json!({ "props": [{ "name": "名称", "type": "title" }, { "name": "工时", "type": "number" }] })).await;
    let (st, v) = schema_ops(
        &s,
        &c,
        &tasks.id,
        json!([{ "op": "add", "name": "项目", "type": "relation", "db_id": people.id, "twoway": true, "pair_name": "任务" }]),
    )
    .await;
    assert_eq!(st, 200, "{v}");
    refresh(&s, &c, &mut tasks).await;
    refresh(&s, &c, &mut people).await;
    // 对方库里有了反向的那一半
    let mirror = people.schema["props"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "任务")
        .unwrap()
        .clone();
    assert_eq!(mirror["relation"]["mirror_of"], tasks.key("项目").as_str());
    let (pids, _) = add(
        &s,
        &c,
        &people.id,
        json!([{ "props": { "名称": "火箭" } }, { "props": { "名称": "潜艇" } }]),
    )
    .await;
    // 关联写对方行的名称
    let (tids, _) = add(
        &s,
        &c,
        &tasks.id,
        json!([{ "props": { "名称": "设计", "工时": 3, "项目": "火箭" } }, { "props": { "名称": "造", "工时": 5, "项目": ["火箭"] } }, { "props": { "名称": "游", "工时": 1, "项目": "潜艇" } }]),
    )
    .await;
    let all = rows(&s, &c, &tasks.id, None).await;
    assert_eq!(
        row(&all, &tids[0])["props"][tasks.key("项目")],
        json!([pids[0]])
    );
    let (st, _) = patch(
        &s,
        &c,
        &tasks.id,
        json!([{ "row_id": tids[0], "set": { "项目": "不存在的项目" } }]),
    )
    .await;
    assert_eq!(st, 400);

    // 汇总：沿反向关联取工时求和
    let (st, v) = schema_ops(
        &s,
        &c,
        &people.id,
        json!([{ "op": "add", "name": "总工时", "type": "rollup", "relation": "任务", "prop": "工时", "fn": "sum" }]),
    )
    .await;
    assert_eq!(st, 200, "{v}");
    refresh(&s, &c, &mut people).await;
    let (_, q) = json(
        c.post(s.api(&format!("/databases/{}/query", people.id)))
            .json(&json!({})),
    )
    .await;
    let got: Vec<(Value, Value)> = q["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["id"].clone(), r["computed"][people.key("总工时")].clone()))
        .collect();
    assert_eq!(
        got,
        vec![(json!(pids[0]), json!(8)), (json!(pids[1]), json!(1))]
    );
    // 对方改了工时：再查就是新的
    patch(
        &s,
        &c,
        &tasks.id,
        json!([{ "row_id": tids[2], "set": { "工时": 4 } }]),
    )
    .await;
    let (_, q) = json(
        c.post(s.api(&format!("/databases/{}/query", people.id)))
            .json(&json!({ "sorts": [{ "prop": "总工时", "dir": "asc" }] })),
    )
    .await;
    assert_eq!(q["rows"][0]["computed"][people.key("总工时")], 4);

    // 删掉这边的关联：对方那一半也去掉
    let (st, _) = schema_ops(
        &s,
        &c,
        &tasks.id,
        json!([{ "op": "delete", "prop": "项目" }]),
    )
    .await;
    assert_eq!(st, 200);
    refresh(&s, &c, &mut people).await;
    assert!(!people.schema.to_string().contains("mirror_of"));
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn csv_import_export_and_compression() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let csv = "\u{feff}Name,价格,日期,完成,状态,网址,邮箱,标签\n\
               苹果,\"1,200\",2026-11-01,是,进行中,https://a.com,a@b.cn,\"红,甜\"\n\
               香蕉,3.5,2026/11/02,否,已完成,https://b.com,c@d.cn,甜\n\
               梨,,2026-11-03,是,进行中,,,脆\n\
               桃,7,2026-11-04,否,进行中,https://c.com,e@f.cn,\"甜,脆\"\n\
               ,,,,,,,\n";
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(csv.as_bytes().to_vec()).file_name("水果.csv"),
    );
    let (st, v) = json(
        c.post(s.api(&format!("/workspaces/{}/databases/import", s.ws_id())))
            .multipart(form),
    )
    .await;
    assert_eq!(st, 201, "{v}");
    assert_eq!(v["page"]["title"], "水果");
    let types: Vec<(&str, &str)> = v["schema"]["props"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["name"].as_str().unwrap(), p["type"].as_str().unwrap()))
        .collect();
    assert_eq!(
        types,
        vec![
            ("Name", "title"),
            ("价格", "number"),
            ("日期", "date"),
            ("完成", "checkbox"),
            ("状态", "select"),
            ("网址", "url"),
            ("邮箱", "email"),
            ("标签", "multi_select"),
        ]
    );
    let db = Db {
        id: v["page"]["id"].as_str().unwrap().into(),
        schema: v["schema"].clone(),
    };
    let all = rows(&s, &c, &db.id, None).await;
    assert_eq!(all["rows"].as_array().unwrap().len(), 4, "空行跳过");
    assert_eq!(all["rows"][0]["props"][db.key("价格")], 1200);
    assert_eq!(
        all["rows"][1]["props"][db.key("日期")]["start"],
        "2026-11-02"
    );
    assert_eq!(
        all["rows"][0]["props"][db.key("标签")]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // 导出：表头是属性名，值是给人看的文字
    let res = c
        .get(s.api(&format!("/databases/{}/export.csv", db.id)))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 200);
    assert!(
        res.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/csv")
    );
    assert!(
        res.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .contains("UTF-8''")
    );
    let out = res.text().await.unwrap();
    let mut lines = out.trim_start_matches('\u{feff}').lines();
    assert_eq!(
        lines.next().unwrap(),
        "Name,价格,日期,完成,状态,网址,邮箱,标签"
    );
    let first = lines.next().unwrap();
    assert!(
        first.starts_with("苹果,1200,2026-11-01,是,进行中,https://a.com,a@b.cn,"),
        "{first}"
    );
    // 再导入一次得到一样的类型
    let (st, v2) = json(
        c.post(s.api(&format!(
            "/workspaces/{}/databases/import?title=又一次",
            s.ws_id()
        )))
        .header("content-type", "text/csv")
        .body(out.clone()),
    )
    .await;
    assert_eq!(st, 201, "{v2}");
    assert_eq!(v2["schema"]["props"][1]["type"], "number");

    // 行的全量按 Accept-Encoding 压缩
    let mut compressed = 0;
    for (enc, want) in [("br", "br"), ("gzip", "gzip"), ("identity", "")] {
        let path = format!("/api/v1/databases/{}/rows", db.id);
        let (st, hs, body) = raw_get(s.addr, &path, &[("Accept-Encoding", enc)]).await;
        assert_eq!(st, 200);
        let got = hs
            .iter()
            .find(|(k, _)| k == "content-encoding")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        // 太小的不压
        if body.len() < 1024 && got.is_empty() {
            continue;
        }
        assert_eq!(got, want);
        compressed += usize::from(!want.is_empty());
        let raw = match want {
            "br" => {
                let mut o = Vec::new();
                brotli_decompress(&body, &mut o);
                o
            }
            "gzip" => {
                let mut o = Vec::new();
                flate2::read::GzDecoder::new(&body[..])
                    .read_to_end(&mut o)
                    .unwrap();
                o
            }
            _ => body,
        };
        let v: Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(v["rows"].as_array().unwrap().len(), 4);
    }
    assert_eq!(compressed, 2);
    s.stop().await;
}

fn brotli_decompress(data: &[u8], out: &mut Vec<u8>) {
    let mut r = brotli::Decompressor::new(data, 4096);
    r.read_to_end(out).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn embed_unfurl_and_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let db = create_db(
        &s,
        &c,
        "嵌入的库",
        json!({ "props": [{ "name": "名称", "type": "title" }] }),
    )
    .await;
    let (_, info) = json(c.get(s.api(&format!("/databases/{}", db.id)))).await;
    let view = info["views"][0]["id"].as_str().unwrap().to_string();
    for url in [
        format!("/p/{}?view={view}", db.id),
        format!("https://docs.example.com/p/{}?view={view}", db.id),
    ] {
        let (st, v) = json(c.get(s.api("/unfurl")).query(&[("url", url.as_str())])).await;
        assert_eq!(st, 200, "{v}");
        assert_eq!(v["kind"], "database");
        assert_eq!(v["db_id"], db.id.as_str());
        assert_eq!(v["view_id"], view.as_str());
        assert_eq!(v["title"], "嵌入的库");
        assert_eq!(v["views"][0]["type"], "table");
    }
    // 不是库的页面、不是库的接口
    let page = page_with(&s, &c, "普通页", "正文").await;
    let (st, _) = json(c.get(s.api(&format!("/databases/{page}")))).await;
    assert_eq!(st, 404);
    let (st, _) = json(
        c.post(s.api(&format!("/databases/{page}/rows")))
            .json(&json!({ "rows": [{ "props": {} }] })),
    )
    .await;
    assert_eq!(st, 404);
    s.stop().await;
}

/// 只读的 Agent 不能写库；可写的 Agent 经 MCP 建库、加行、查、改、改表结构和撤销，read_page 读行先列属性。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcp_tools() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let agents = s.api(&format!("/workspaces/{}/agents", s.ws_id()));
    let (_, token, _) = make_agent(c.post(&agents), "表格助手", "write", None).await;
    let (_, read_tok, _) = make_agent(c.post(&agents), "只读助手", "read", None).await;
    let m = mcp_client(&s, &token).await.unwrap();

    let (t, err) = call(
        &m,
        "db_create",
        json!({ "title": "读书清单", "props": [
            { "name": "书名", "type": "title" },
            { "name": "状态", "type": "status" },
            { "name": "评分", "type": "number" },
            { "name": "读完", "type": "date" },
            { "name": "双倍评分", "type": "formula", "formula": "prop(\"评分\") * 2" }
        ]}),
    )
    .await;
    assert!(!err, "{t}");
    let db = field(&t, "db_id");
    assert!(t.contains("书名（标题）"), "{t}");
    assert!(t.contains("状态（状态：未开始 / 进行中 / 已完成）"), "{t}");
    // 表结构不对：不留下半个库
    let (t, err) = call(
        &m,
        "db_create",
        json!({ "title": "坏的", "props": [{ "name": "x", "type": "不存在的类型" }] }),
    )
    .await;
    assert!(err, "{t}");
    let (_, list) = json(c.get(s.api(&format!("/workspaces/{}/pages", s.ws_id())))).await;
    assert_eq!(list["pages"].as_array().unwrap().len(), 1);

    let (t, err) = call(
        &m,
        "db_add",
        json!({ "db_id": db, "rows": [
            { "书名": "三体", "状态": "已完成", "评分": 9, "读完": "2026-10-01", "body": "很好看" },
            { "书名": "红楼梦", "状态": "进行中", "评分": 10 },
            { "书名": "围城", "评分": 7 }
        ]}),
    )
    .await;
    assert!(!err, "{t}");
    assert!(t.contains("已加 3 行"), "{t}");
    let first = t
        .lines()
        .find_map(|l| l.strip_prefix("- "))
        .unwrap()
        .to_string();

    let (t, err) = call(&m, "db_query", json!({ "db_id": db, "filter": { "op": "and", "items": [{ "prop": "评分", "cond": "gte", "value": 9 }] }, "sorts": [{ "prop": "评分", "dir": "desc" }] })).await;
    assert!(!err, "{t}");
    assert!(
        t.contains("| row_id | 书名 | 状态 | 评分 | 读完 | 双倍评分 |"),
        "{t}"
    );
    assert!(t.contains("| 红楼梦 | 进行中 | 10 |"), "{t}");
    assert!(t.contains("| 18 |"), "公式算出来：{t}");
    assert!(t.contains("共 2 行"), "{t}");
    let red = t.find("红楼梦").unwrap();
    assert!(red < t.find("三体").unwrap(), "按评分倒序");

    let (t, err) = call(
        &m,
        "db_update",
        json!({ "rows": [{ "row_id": first, "set": { "状态": "进行中", "评分": null } }] }),
    )
    .await;
    assert!(!err, "{t}");
    assert!(t.contains("已改 2 格"), "{t}");
    let (t, err) = call(
        &m,
        "db_update",
        json!({ "rows": [{ "row_id": first, "set": { "双倍评分": 1 } }] }),
    )
    .await;
    assert!(err, "公式不能写：{t}");

    // read_page 读行：先列属性，再是正文
    let (t, err) = call(&m, "read_page", json!({ "page_id": first })).await;
    assert!(!err, "{t}");
    let props_at = t.find("- 状态：进行中").unwrap_or_else(|| panic!("{t}"));
    assert!(props_at < t.find("很好看").unwrap(), "{t}");
    assert!(!t.contains("- 评分："), "空的不列：{t}");
    let (t, _) = call(&m, "read_page", json!({ "page_id": db })).await;
    assert!(
        t.contains("这是一个数据库") && t.contains("书名（标题）"),
        "{t}"
    );

    // 改表结构、撤销
    let (t, err) = call(
        &m,
        "db_schema",
        json!({ "db_id": db, "ops": [{ "op": "delete", "prop": "评分" }] }),
    )
    .await;
    assert!(!err, "{t}");
    assert!(t.contains("双倍评分"), "{t}");
    assert!(t.contains("注意"), "公式用到的列删了要提醒：{t}");
    let seq: i64 = t
        .split("seq: ")
        .nth(1)
        .unwrap()
        .split(['；', '）'])
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let (t, err) = call(
        &m,
        "db_schema",
        json!({ "db_id": db, "ops": [{ "op": "undo", "seq": seq }] }),
    )
    .await;
    assert!(!err, "{t}");
    assert!(t.contains("评分（数字）"), "{t}");
    let (t, _) = call(&m, "db_query", json!({ "db_id": db, "search": "红楼" })).await;
    assert!(t.contains("| 10 |"), "撤销后值回来了：{t}");

    // 只读的 Agent
    let r = mcp_client(&s, &read_tok).await.unwrap();
    let (t, err) = call(&r, "db_query", json!({ "db_id": db })).await;
    assert!(!err, "{t}");
    let (t, err) = call(
        &r,
        "db_add",
        json!({ "db_id": db, "rows": [{ "书名": "x" }] }),
    )
    .await;
    assert!(err, "{t}");
    s.stop().await;
}

/// 手动排序：PATCH rows 的 op 带 after（null 是最前面）；服务器查询的默认顺序跟着变。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn move_rows() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let db = create_db(
        &s,
        &c,
        "库",
        json!({ "props": [{ "name": "名称", "type": "title" }] }),
    )
    .await;
    let (ids, _) = add(&s, &c, &db.id, json!([{ "props": { "名称": "a" } }, { "props": { "名称": "b" } }, { "props": { "名称": "c" } }])).await;
    let order = async || -> (Vec<String>, Vec<String>) {
        let all = rows(&s, &c, &db.id, None).await;
        let (_, q) = json(
            c.post(s.api(&format!("/databases/{}/query", db.id)))
                .json(&json!({})),
        )
        .await;
        let a = all["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["props"][db.key("名称")].as_str().unwrap().to_string())
            .collect();
        let b = q["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["props"][db.key("名称")].as_str().unwrap().to_string())
            .collect();
        (a, b)
    };
    order().await; // 引擎装上
    let (st, v) = patch(&s, &c, &db.id, json!([{ "row_id": ids[2], "after": null }])).await;
    assert_eq!(st, 200, "{v}");
    assert_eq!(
        order().await,
        (
            vec!["c".into(), "a".into(), "b".into()],
            vec!["c".into(), "a".into(), "b".into()]
        )
    );
    let (st, _) = patch(
        &s,
        &c,
        &db.id,
        json!([{ "row_id": ids[2], "after": ids[1] }]),
    )
    .await;
    assert_eq!(st, 200);
    assert_eq!(order().await.1, vec!["a", "b", "c"]);
    let (st, _) = patch(
        &s,
        &c,
        &db.id,
        json!([{ "row_id": ids[0], "after": "nope" }]),
    )
    .await;
    assert_eq!(st, 400);
    s.stop().await;
}

/// 重启后库、行、行的正文都在；引擎缓存是空的，第一次用时从库里装。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let c = http();
    let (db, ids) = {
        let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
        let db = create_db(&s, &c, "库", task_schema()).await;
        let (ids, _) = add(
            &s,
            &c,
            &db.id,
            json!([{ "props": { "任务": "a", "状态": "进行中" }, "body": "正文还在" }]),
        )
        .await;
        s.stop().await;
        (db, ids)
    };
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let (_, q) = json(
        c.post(s.api(&format!("/databases/{}/query", db.id)))
            .json(&json!({ "filter": { "op": "and", "items": [{ "prop": "状态", "cond": "eq", "value": "进行中" }] } })),
    )
    .await;
    assert_eq!(q["ids"], json!(ids));
    let (_, content) = json(c.get(s.api(&format!("/pages/{}/content", ids[0])))).await;
    assert!(content["markdown"].as_str().unwrap().contains("正文还在"));
    let (_, info) = json(c.get(s.api(&format!("/databases/{}", db.id)))).await;
    assert_eq!(info["seq"], 1);
    s.stop().await;
}

/// 网页“新建数据库”不带表结构：默认的标题列、视图名按请求的人的语言（没设过就按 Accept-Language）；加状态列的默认选项也是。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn default_names_follow_language() {
    let dir = tempfile::tempdir().unwrap();
    let s = Server::start(dir.path(), AuthConfig::None, fast_hub()).await;
    let c = http();
    let ws = s.ws_id();
    let (st, en) = json(
        c.post(s.api(&format!("/workspaces/{ws}/databases")))
            .header("accept-language", "en-US,en;q=0.9")
            .json(&json!({ "title": "Tasks" })),
    )
    .await;
    assert_eq!(st, 201, "{en}");
    assert_eq!(en["schema"]["props"][0]["name"], "Name", "{en}");
    assert_eq!(en["views"][0]["name"], "Table", "{en}");
    let id = en["page"]["id"].as_str().unwrap();
    let (st, v) = json(
        c.patch(s.api(&format!("/databases/{id}/schema")))
            .header("accept-language", "en")
            .json(&json!({ "ops": [{ "op": "add", "name": "Status", "type": "status" }] })),
    )
    .await;
    assert_eq!(st, 200, "{v}");
    let status = v["schema"]["props"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["type"] == "status")
        .unwrap()
        .clone();
    let names: Vec<&str> = status["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Not started", "In progress", "Done"]);
    // 中文（默认）照旧
    let (_, zh) = json(
        c.post(s.api(&format!("/workspaces/{ws}/databases")))
            .json(&json!({ "title": "任务" })),
    )
    .await;
    assert_eq!(zh["schema"]["props"][0]["name"], "名称", "{zh}");
    assert_eq!(zh["views"][0]["name"], "表格", "{zh}");
}
