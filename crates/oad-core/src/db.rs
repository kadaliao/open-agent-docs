//! 数据库：一种页面（kind = database），行也是页面（kind = row，parent 是库，有自己的正文）。
//! 属性存在 db_rows（每格最后写入为准，改动记 db_ops），表结构、视图存在 databases；算值（规范化、公式、筛选排序）交给 oad-db，见 [`crate::dbengine`]。
//! 见 docs/phase4-contract.md。
use crate::dbengine as eng;
use crate::events::Event;
use crate::model::*;
use crate::{Core, Error, PageDoc, Result};
use oad_db::schema::{PropType, View};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// 短随机串（属性 key、视图 id）。
pub fn short_id(n: usize) -> String {
    const A: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    (0..n)
        .map(|_| A[(rand::random::<u32>() % A.len() as u32) as usize] as char)
        .collect()
}

/// 新库的表结构：只有标题。
pub fn default_schema() -> Value {
    json!({ "props": [{ "key": short_id(4), "name": "名称", "type": "title" }], "next_id": 1 })
}

pub fn default_views() -> Value {
    json!([{ "id": short_id(6), "name": "表格", "type": "table", "filter": { "op": "and", "items": [] },
             "sorts": [], "hidden": [], "order": [], "widths": {} }])
}

/// 建库（POST /workspaces/{ws}/databases 的正文）。
#[derive(Deserialize, Default, Clone, Debug, utoipa::ToSchema)]
pub struct NewDb {
    pub title: Option<String>,
    pub parent_id: Option<String>,
    /// 放在同级里这个页面后面
    pub after: Option<String>,
    /// 表结构 `{ props: [{ name, type, … }] }`，和 DbSchema 一样，但属性可以不给 key（自动分配），
    /// 选项可以只写名字（`options: ["做", "完"]`）。不给只有标题
    #[schema(value_type = Option<Object>)]
    pub schema: Option<Value>,
    /// 视图（和 DbView 一样，id 可以不给）；不给是一个表格视图
    #[schema(value_type = Option<Vec<Object>>)]
    pub views: Option<Value>,
}

/// 库的信息（GET /databases/{id}）。
#[derive(Serialize, Clone, Debug, utoipa::ToSchema)]
pub struct DbInfo {
    pub page: Page,
    #[schema(value_type = crate::dbdoc::DbSchema)]
    pub schema: Value,
    #[schema(value_type = Vec<crate::dbdoc::DbView>)]
    pub views: Value,
    pub seq: i64,
}

/// 增量（GET /databases/{id}/rows）。
#[derive(Serialize, Clone, Debug, utoipa::ToSchema)]
pub struct RowsDelta {
    pub seq: i64,
    pub rows: Vec<DbRow>,
    pub deleted: Vec<String>,
}

/// 加一行：属性（存的样子或名字）、排在谁后面、正文（Markdown）。
#[derive(Clone, Debug, Default)]
pub struct RowInput {
    pub props: Map<String, Value>,
    pub after: Option<String>,
    pub body: Option<String>,
}

impl Core {
    pub(crate) fn emit_db(&self, workspace_id: &str, name: &'static str, data: Value) {
        self.events.emit(Event {
            workspace_id: workspace_id.to_string(),
            name,
            data,
            user_id: None,
        });
    }

    /// 库页面和权限。
    pub(crate) async fn db_for(
        &self,
        db_id: &str,
        who: &Principal,
        write: bool,
    ) -> Result<(Page, DbMeta)> {
        let (page, _) = self.page_for(db_id, who, write).await?;
        if page.kind != "database" {
            return Err(Error::NotFound(format!("{db_id} 不是数据库")));
        }
        let meta = self
            .storage
            .database(db_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("数据库 {db_id} 不存在")))?;
        Ok((page, meta))
    }

    /// 建库。schema 不给就只有标题；给了的话检查一遍（第一个必须是标题、key 补全）。
    pub async fn create_database(
        &self,
        workspace_id: &str,
        who: &Principal,
        d: NewDb,
    ) -> Result<DbInfo> {
        let NewDb {
            title,
            parent_id,
            after,
            schema,
            views,
        } = d;
        self.require(workspace_id, who, true).await?;
        let schema = match schema {
            Some(s) => eng::check_schema(s)?,
            None => default_schema(),
        };
        let views = match views {
            Some(v) if v.as_array().is_some_and(|a| !a.is_empty()) => eng::check_views(v, &schema)?,
            _ => default_views(),
        };
        let title = title.unwrap_or_default();
        let doc = PageDoc::new(oad_doc::server_client_id());
        doc.set_title(&title);
        let page = self
            .storage
            .create_database(
                NewPage {
                    workspace_id: workspace_id.into(),
                    parent_id,
                    place: after.map(Placement::After).unwrap_or(Placement::Last),
                    title,
                    icon: None,
                    created_by: Some(who.user_id.clone()),
                    initial_state: doc.encode_state(),
                    kind: "database".into(),
                },
                schema.clone(),
                views.clone(),
            )
            .await?;
        self.events.page(crate::EventKind::Created, page.clone());
        Ok(DbInfo {
            page,
            schema,
            views,
            seq: 0,
        })
    }

    pub async fn database_info(&self, db_id: &str, who: &Principal) -> Result<DbInfo> {
        let (page, meta) = self.db_for(db_id, who, false).await?;
        Ok(DbInfo {
            page,
            schema: meta.schema,
            views: meta.views,
            seq: meta.seq,
        })
    }

    /// 行：不给 since 是全部；给了是之后改过的和删掉的。
    pub async fn db_rows(
        &self,
        db_id: &str,
        who: &Principal,
        since: Option<i64>,
    ) -> Result<RowsDelta> {
        self.db_for(db_id, who, false).await?;
        let (seq, rows, deleted) = self.storage.db_rows(db_id, since).await?;
        Ok(RowsDelta { seq, rows, deleted })
    }

    /// 加行。属性可以写存的样子，也可以写名字（Agent），都规范化成存的样子；标题同时是行页面的标题；有 body 的写进正文。
    pub async fn add_db_rows(
        &self,
        db_id: &str,
        who: &Principal,
        rows: Vec<RowInput>,
    ) -> Result<(Vec<String>, i64)> {
        let (page, meta) = self.db_for(db_id, who, true).await?;
        if rows.is_empty() {
            return Err(Error::BadRequest("没有要加的行".into()));
        }
        if rows.len() > 10_000 {
            return Err(Error::BadRequest("一次最多加 1 万行".into()));
        }
        let mut rows = rows;
        {
            let mut c = self.engine(&page, &meta).await?;
            for (i, r) in rows.iter_mut().enumerate() {
                r.props = eng::normalize_props(&mut c, &r.props, true)
                    .map_err(|e| Error::BadRequest(format!("第 {} 行：{}", i + 1, e)))?;
            }
            let added = c.new_options.then(|| c.schema_value());
            drop(c);
            self.save_new_options(&page, &meta, added, who).await?;
        }
        self.add_db_rows_raw(&page, who, rows).await
    }

    /// 加已经规范化好的行。
    pub(crate) async fn add_db_rows_raw(
        &self,
        page: &Page,
        who: &Principal,
        rows: Vec<RowInput>,
    ) -> Result<(Vec<String>, i64)> {
        let meta = self
            .storage
            .database(&page.id)
            .await?
            .ok_or_else(|| Error::NotFound("数据库不存在".into()))?;
        let title_key = eng::title_key(&meta.schema)?;
        let mut news = Vec::with_capacity(rows.len());
        let mut bodies = Vec::new();
        for r in rows {
            let title = r
                .props
                .get(&title_key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let doc = PageDoc::new(oad_doc::server_client_id());
            doc.set_title(&title);
            news.push(NewRow {
                props: r.props,
                after: r.after,
                title,
                initial_state: doc.encode_state(),
            });
            bodies.push(r.body);
        }
        let (ids, seq) = self
            .storage
            .add_db_rows(&page.id, news, &eng::person_ref(who))
            .await?;
        for (id, body) in ids.iter().zip(bodies) {
            if let Some(md) = body.filter(|b| !b.trim().is_empty()) {
                self.set_content(id, who, oad_markdown::parse(&md), None)
                    .await?;
            }
        }
        self.emit_db(
            &page.workspace_id,
            "db.changed",
            json!({ "db_id": page.id, "seq": seq }),
        );
        Ok((ids, seq))
    }

    /// 改格子（每格最后写入为准）。值可以是存的样子，也可以是名字；只读的属性（公式、汇总、自动的）不能写。
    /// 改标题同时改行页面的标题（经同步中心，网页马上看到）。
    pub async fn patch_db_rows(
        &self,
        db_id: &str,
        who: &Principal,
        ops: Vec<CellOp>,
    ) -> Result<(i64, Vec<CellChange>)> {
        let (page, meta) = self.db_for(db_id, who, true).await?;
        if ops.is_empty() {
            return Ok((meta.seq, vec![]));
        }
        let title_key = eng::title_key(&meta.schema)?;
        let mut clean = Vec::with_capacity(ops.len());
        {
            let mut c = self.engine(&page, &meta).await?;
            for op in ops {
                let set = eng::normalize_props(&mut c, &op.set, true)?;
                clean.push(CellOp {
                    row_id: op.row_id,
                    set,
                    after: op.after,
                });
            }
            let added = c.new_options.then(|| c.schema_value());
            drop(c);
            self.save_new_options(&page, &meta, added, who).await?;
        }
        let (seq, changes) = self
            .storage
            .patch_db_rows(db_id, &title_key, clean, &eng::person_ref(who))
            .await?;
        for c in changes.iter().filter(|c| c.prop == title_key) {
            let t = c
                .new
                .as_ref()
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            self.hub.set_title(&c.row_id, t).await?;
        }
        if !changes.is_empty() {
            self.emit_db(
                &page.workspace_id,
                "db.changed",
                json!({ "db_id": db_id, "seq": seq }),
            );
        }
        Ok((seq, changes))
    }

    pub async fn delete_db_rows(
        &self,
        db_id: &str,
        who: &Principal,
        ids: Vec<String>,
    ) -> Result<i64> {
        let (page, _) = self.db_for(db_id, who, true).await?;
        let seq = self
            .storage
            .delete_db_rows(db_id, ids.clone(), &eng::person_ref(who))
            .await?;
        self.emit_db(
            &page.workspace_id,
            "db.changed",
            json!({ "db_id": db_id, "seq": seq }),
        );
        self.storage
            .set_deleted_by(ids, &eng::person_ref(who))
            .await?;
        Ok(seq)
    }

    /// 规范化时新建了选项：存回表结构（记一条表结构改动，能撤销）。
    async fn save_new_options(
        &self,
        page: &Page,
        meta: &DbMeta,
        after: Option<Value>,
        who: &Principal,
    ) -> Result<()> {
        let Some(after) = after else { return Ok(()) };
        let w = SchemaWrite {
            schema: after.clone(),
            views: None,
            op: json!([{ "op": "add_options" }]).to_string(),
            before: json!({ "schema": meta.schema }),
            after: json!({ "schema": after }),
            cells: vec![],
        };
        self.storage
            .set_db_schema(&page.id, w, &eng::person_ref(who))
            .await?;
        self.emit_db(&page.workspace_id, "db.schema", json!({ "db_id": page.id }));
        Ok(())
    }

    /// 按视图（或给的条件）查。返回 { total, ids, rows（这一段的行，带算出来的值 computed）, groups?, totals? }。
    pub async fn query_db(&self, db_id: &str, who: &Principal, q: QueryReq) -> Result<Value> {
        let (page, meta) = self.db_for(db_id, who, false).await?;
        let view = build_view(&meta.views, &q)?;
        let c = self.engine(&page, &meta).await?;
        let r = c.eng.query(&view).map_err(|e| Error::BadRequest(e.msg))?;
        let rows: Vec<Value> = r
            .ids
            .iter()
            .filter_map(|id| {
                let d = c.eng.row(id)?;
                let mut v = serde_json::to_value(d).ok()?;
                v["computed"] = Value::Object(c.eng.computed(id));
                Some(v)
            })
            .collect();
        let mut out = serde_json::to_value(&r).unwrap_or_default();
        out["rows"] = Value::Array(rows);
        out["seq"] = json!(meta.seq);
        Ok(out)
    }

    /// 查，结果是给人看的文字（列名、每格的名字）：MCP 的 db_query 用。
    pub async fn query_shown(
        &self,
        db_id: &str,
        who: &Principal,
        q: QueryReq,
    ) -> Result<(Page, Shown)> {
        let (page, meta) = self.db_for(db_id, who, false).await?;
        let lang = self.lang_of(&who.user_id).await;
        let offset = q.offset.unwrap_or(0);
        let view = build_view(&meta.views, &q)?;
        let c = self.engine(&page, &meta).await?;
        let r = c.eng.query(&view).map_err(|e| Error::BadRequest(e.msg))?;
        // 视图里隐藏的列不列；按钮不列
        let hidden: Vec<String> = q
            .view_id
            .as_deref()
            .and_then(|v| find_view(&meta.views, v))
            .and_then(|v| v["hidden"].as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let cols: Vec<_> = c
            .eng
            .schema()
            .props
            .iter()
            .filter(|p| p.kind != PropType::Button && !hidden.contains(&p.key))
            .cloned()
            .collect();
        let rows = r
            .ids
            .iter()
            .map(|id| ShownRow {
                id: id.clone(),
                cells: cols
                    .iter()
                    .map(|p| {
                        eng::localize_display(
                            &c.eng.display(&p.key, &c.eng.value(id, &p.key)),
                            lang,
                        )
                    })
                    .collect(),
            })
            .collect();
        let shown = Shown {
            columns: cols.iter().map(|p| p.name.clone()).collect(),
            rows,
            total: r.total,
            offset,
        };
        Ok((page, shown))
    }

    /// 行页面上面列的属性：[(名字, 文字)]，空的不列。不是行就是 None。
    pub async fn row_props_text(
        &self,
        row_id: &str,
        who: &Principal,
    ) -> Result<Option<(Page, Vec<(String, String)>)>> {
        let (page, _) = self.page_for(row_id, who, false).await?;
        if page.kind != "row" {
            return Ok(None);
        }
        let lang = self.lang_of(&who.user_id).await;
        let Some(db) = page.parent_id.clone() else {
            return Ok(None);
        };
        let (dbp, meta) = self.db_for(&db, who, false).await?;
        let c = self.engine(&dbp, &meta).await?;
        let mut out = Vec::new();
        for p in &c.eng.schema().props {
            if matches!(p.kind, PropType::Title | PropType::Button) {
                continue;
            }
            let t =
                eng::localize_display(&c.eng.display(&p.key, &c.eng.value(row_id, &p.key)), lang);
            if !t.is_empty() {
                out.push((p.name.clone(), t));
            }
        }
        Ok(Some((dbp, out)))
    }

    /// 按一行上的按钮：按设置改属性（`set: { 属性: 值 }`，名字也认；已经删了的属性、认不出的值跳过），
    /// 有 `comment` 的在这一行上发一条评论（`agent` 给了就叫那个 Agent）。返回 { seq, changes, thread_id? }。
    pub async fn press_button(
        &self,
        db_id: &str,
        who: &Principal,
        row_id: &str,
        key: &str,
    ) -> Result<Value> {
        let (page, meta) = self.db_for(db_id, who, true).await?;
        let schema = eng::parse_schema(&meta.schema)?;
        let btn = schema
            .find(key)
            .filter(|p| p.kind == PropType::Button)
            .ok_or_else(|| Error::NotFound(format!("没有按钮「{key}」")))?
            .clone();
        if self
            .storage
            .db_row(row_id)
            .await?
            .is_none_or(|r| r.db_id != db_id || r.deleted_at.is_some())
        {
            return Err(Error::NotFound(format!("库里没有行 {row_id}")));
        }
        let mut set = Map::new();
        if let Some(Value::Object(want)) = btn.extra.get("set") {
            let c = self.engine(&page, &meta).await?;
            for (k, v) in want {
                let Some(p) = c.eng.find_prop(k) else {
                    continue;
                };
                if p.is_writable() && c.eng.normalize(&p.key, v).is_ok() {
                    set.insert(p.key.clone(), v.clone());
                }
            }
        }
        let (seq, changes) = if set.is_empty() {
            (meta.seq, vec![])
        } else {
            self.patch_db_rows(
                db_id,
                who,
                vec![CellOp {
                    row_id: row_id.into(),
                    set,
                    after: None,
                }],
            )
            .await?
        };
        let mut out = json!({ "seq": seq, "changes": changes });
        if let Some(text) = btn
            .extra
            .get("comment")
            .and_then(Value::as_str)
            .filter(|t| !t.trim().is_empty())
        {
            let notify = match btn.extra.get("agent").and_then(Value::as_str) {
                Some(a) => {
                    crate::NotifySpec::Only(vec![a.trim_start_matches("agent:").to_string()])
                }
                None => crate::NotifySpec::Auto,
            };
            let t = self.create_thread(row_id, who, None, text, notify).await?;
            out["thread_id"] = json!(t.id);
        }
        Ok(out)
    }

    /// 库的表结构给 Agent 看（关联写库名）。
    pub async fn schema_text(
        &self,
        db_id: &str,
        who: &Principal,
    ) -> Result<(Page, DbMeta, String)> {
        let (page, meta) = self.db_for(db_id, who, false).await?;
        let s = eng::parse_schema(&meta.schema)?;
        let mut names = std::collections::HashMap::new();
        for p in &s.props {
            if let Some(r) = &p.relation
                && let Some(x) = self.storage.page(&r.db_id).await?
                && x.workspace_id == page.workspace_id
            {
                names.insert(r.db_id.clone(), x.title);
            }
        }
        let lang = self.lang_of(&who.user_id).await;
        let t = eng::schema_text(&s, lang, &|id| names.get(id).cloned());
        Ok((page, meta, t))
    }

    /// 库的历史（新的在前，最多 limit 条）：格子改动 `cell`、加 / 删行 `row`、表结构改动 `schema`（只看全部时有）。
    /// 可以只看一行、一个属性（写名字也行）。
    pub async fn db_history(
        &self,
        db_id: &str,
        who: &Principal,
        h: HistoryQuery<'_>,
        lang: crate::i18n::Lang,
    ) -> Result<Value> {
        let HistoryQuery {
            row,
            prop,
            limit,
            cursor,
        } = h;
        let before = crate::parse_cursor(cursor)?;
        let (_, meta) = self.db_for(db_id, who, false).await?;
        let schema = eng::parse_schema(&meta.schema)?;
        let prop = prop.map(|p| {
            schema
                .find(p)
                .map(|x| x.key.clone())
                .unwrap_or_else(|| p.to_string())
        });
        let limit = limit.clamp(1, 1000);
        let ops = self
            .storage
            .db_ops(db_id, row, prop.as_deref(), limit + 1, before)
            .await?;
        let schema_ops = if row.is_none() && prop.is_none() {
            self.storage.db_schema_ops(db_id, limit + 1, before).await?
        } else {
            vec![]
        };
        // 撤销过的：之后有一条 undo 指向它
        let undone: std::collections::HashSet<i64> = schema_ops
            .iter()
            .filter_map(|o| serde_json::from_str::<Value>(&o.op).ok())
            .filter_map(|v| v[0]["seq"].as_i64().filter(|_| v[0]["op"] == "undo"))
            .collect();
        let mut people: std::collections::HashMap<String, Option<Value>> = Default::default();
        let mut titles: std::collections::HashMap<String, String> = Default::default();
        let mut items: Vec<(i64, i64, Value)> = Vec::new();
        for o in ops {
            let by = match &o.by {
                Some(b) if !b.is_empty() => {
                    if !people.contains_key(b) {
                        let p = self.person_json(b).await;
                        people.insert(b.clone(), p);
                    }
                    people[b].clone().unwrap_or(Value::String(b.clone()))
                }
                _ => Value::Null,
            };
            if !titles.contains_key(&o.row_id) {
                let t = self
                    .storage
                    .page(&o.row_id)
                    .await?
                    .map(|p| p.title)
                    .unwrap_or_default();
                titles.insert(o.row_id.clone(), t);
            }
            let kind = if o.prop == "*" { "row" } else { "cell" };
            let mut v = json!({
                "seq": o.seq,
                "kind": kind,
                "row_id": o.row_id,
                "row_title": titles[&o.row_id],
                "by": by,
                "at": o.at,
            });
            if kind == "row" {
                v["summary"] = json!(self.t(
                    lang,
                    if o.old.is_none() {
                        "加了一行"
                    } else {
                        "删了一行"
                    },
                    &[]
                ));
            } else {
                v["prop"] = json!(o.prop);
                v["prop_name"] = json!(
                    schema
                        .prop(&o.prop)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| o.prop.clone())
                );
                v["old"] = o.old.unwrap_or(Value::Null);
                v["new"] = o.new.unwrap_or(Value::Null);
            }
            items.push((o.seq, o.at, v));
        }
        for o in schema_ops {
            let ops: Value = serde_json::from_str(&o.op).unwrap_or_default();
            if ops[0]["op"] == "internal" {
                continue;
            }
            let changes = o.before.get("changes").cloned().unwrap_or(json!([]));
            let by = match &o.by {
                Some(b) if !b.is_empty() => self
                    .person_json(b)
                    .await
                    .unwrap_or(Value::String(b.clone())),
                _ => Value::Null,
            };
            items.push((
                o.seq,
                o.at,
                json!({
                    "seq": o.seq,
                    "kind": "schema",
                    "summary": schema_summary(&ops, &changes, |zh, args| self.t(lang, zh, args)),
                    "changes": changes,
                    "cells": o.before.get("cells").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0),
                    "by": by,
                    "at": o.at,
                    "undone": undone.contains(&o.seq),
                }),
            ));
        }
        items.sort_by_key(|x| std::cmp::Reverse((x.0, x.1)));
        // 多了一条说明还有：按 seq 整组切（同一次改动的格子不分到两页），下一页从最后一组之前开始
        let mut next = None;
        if items.len() > limit {
            items.truncate(limit);
            let last = items.last().map(|x| x.0).unwrap_or(0);
            if items.iter().any(|x| x.0 != last) {
                items.retain(|x| x.0 != last);
                next = Some(last + 1);
            } else {
                next = Some(last);
            }
        }
        Ok(json!({
            "items": items.into_iter().map(|x| x.2).collect::<Vec<_>>(),
            "seq": meta.seq,
            "next_cursor": next.map(|n| n.to_string()),
        }))
    }

    /// 撤销第 seq 次改动：表结构改动走表结构的撤销；格子改动把值换回去（加的行删掉、删的行放回），之后又被改过的格子不动。
    pub async fn undo_db(&self, db_id: &str, who: &Principal, seq: i64) -> Result<Value> {
        let (page, meta) = self.db_for(db_id, who, true).await?;
        if self.storage.db_schema_op(db_id, seq).await?.is_some() {
            return self.undo_schema(db_id, who, seq).await;
        }
        let title_key = eng::title_key(&meta.schema)?;
        let (new_seq, titles, back) = self
            .storage
            .undo_db_ops(db_id, seq, &title_key, &eng::person_ref(who))
            .await?;
        for (row, t) in titles {
            self.hub.set_title(&row, t).await?;
        }
        // 放回来的行：正文重新进搜索
        for row in back {
            self.search.touch_page(&row);
        }
        self.emit_db(
            &page.workspace_id,
            "db.changed",
            json!({ "db_id": db_id, "seq": new_seq }),
        );
        Ok(json!({ "seq": new_seq }))
    }

    /// 公开分享里的表格（不查权限，调用方确认过在分享范围里）：按第一个视图（或给的视图）的筛选、排序、隐藏的列，
    /// 返回 (列名, 这一段的行 (id, 每格的文字), 总行数)。
    pub async fn db_public_table(
        &self,
        db_id: &str,
        view_id: Option<&str>,
        offset: usize,
        limit: usize,
        lang: crate::i18n::Lang,
    ) -> Result<(Vec<String>, Vec<(String, Vec<String>)>, usize)> {
        let page = self
            .storage
            .page(db_id)
            .await?
            .filter(|p| p.kind == "database" && p.deleted_at.is_none())
            .ok_or_else(|| Error::NotFound(format!("数据库 {db_id} 不存在")))?;
        let meta = self
            .storage
            .database(db_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("数据库 {db_id} 不存在")))?;
        let vid = view_id
            .filter(|v| find_view(&meta.views, v).is_some())
            .map(str::to_string)
            .or_else(|| meta.views[0]["id"].as_str().map(str::to_string));
        let q = QueryReq {
            view_id: vid.clone(),
            limit: Some(limit),
            offset: Some(offset),
            ..Default::default()
        };
        let view = build_view(&meta.views, &q)?;
        let hidden: Vec<String> = vid
            .as_deref()
            .and_then(|v| find_view(&meta.views, v))
            .and_then(|v| v["hidden"].as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let c = self.engine(&page, &meta).await?;
        let r = c.eng.query(&view).map_err(|e| Error::BadRequest(e.msg))?;
        let cols: Vec<_> = c
            .eng
            .schema()
            .props
            .iter()
            .filter(|p| p.kind != PropType::Button && !hidden.contains(&p.key))
            .cloned()
            .collect();
        let rows = r
            .ids
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    cols.iter()
                        .map(|p| {
                            eng::localize_display(
                                &c.eng.display(&p.key, &c.eng.value(id, &p.key)),
                                lang,
                            )
                        })
                        .collect(),
                )
            })
            .collect();
        Ok((cols.iter().map(|p| p.name.clone()).collect(), rows, r.total))
    }

    /// 行的属性（名字, 文字），空的不列；不查权限。
    pub async fn row_props_public(
        &self,
        row: &Page,
        lang: crate::i18n::Lang,
    ) -> Result<Vec<(String, String)>> {
        let Some(db) = row.parent_id.clone().filter(|_| row.kind == "row") else {
            return Ok(vec![]);
        };
        let Some(dbp) = self.storage.page(&db).await? else {
            return Ok(vec![]);
        };
        let Some(meta) = self.storage.database(&db).await? else {
            return Ok(vec![]);
        };
        let c = self.engine(&dbp, &meta).await?;
        Ok(c.eng
            .schema()
            .props
            .iter()
            .filter(|p| !matches!(p.kind, PropType::Title | PropType::Button))
            .filter_map(|p| {
                let t = eng::localize_display(
                    &c.eng.display(&p.key, &c.eng.value(&row.id, &p.key)),
                    lang,
                );
                (!t.is_empty()).then(|| (p.name.clone(), t))
            })
            .collect())
    }

    // ---- 视图 ----

    async fn save_views(&self, page: &Page, meta: &DbMeta, views: Value) -> Result<Value> {
        let views = eng::check_views(views, &meta.schema)?;
        self.storage.set_db_views(&page.id, views.clone()).await?;
        self.emit_db(&page.workspace_id, "db.schema", json!({ "db_id": page.id }));
        Ok(views)
    }

    /// 整个换掉。
    pub async fn put_views(&self, db_id: &str, who: &Principal, views: Value) -> Result<Value> {
        let (page, meta) = self.db_for(db_id, who, true).await?;
        if !views.as_array().is_some_and(|a| !a.is_empty()) {
            return Err(Error::BadRequest("至少要有一个视图".into()));
        }
        self.save_views(&page, &meta, views).await
    }

    /// 加一个视图，返回它（带上 id）。
    pub async fn add_view(&self, db_id: &str, who: &Principal, mut view: Value) -> Result<Value> {
        let (page, meta) = self.db_for(db_id, who, true).await?;
        let o = view
            .as_object_mut()
            .ok_or_else(|| Error::BadRequest("视图要是对象".into()))?;
        let id = short_id(6);
        o.insert("id".into(), id.clone().into());
        o.entry("type").or_insert("table".into());
        o.entry("name").or_insert("视图".into());
        let mut views = meta.views.as_array().cloned().unwrap_or_default();
        views.push(view);
        let views = self.save_views(&page, &meta, Value::Array(views)).await?;
        Ok(find_view(&views, &id).cloned().unwrap_or_default())
    }

    /// 改一个视图的几项（给了的字段覆盖，null 去掉）。
    pub async fn patch_view(
        &self,
        db_id: &str,
        who: &Principal,
        view_id: &str,
        patch: Value,
    ) -> Result<Value> {
        let (page, meta) = self.db_for(db_id, who, true).await?;
        let mut views = meta.views.as_array().cloned().unwrap_or_default();
        let v = views
            .iter_mut()
            .find(|v| v["id"] == view_id)
            .ok_or_else(|| Error::NotFound(format!("没有视图 {view_id}")))?;
        let p = patch
            .as_object()
            .ok_or_else(|| Error::BadRequest("要是对象".into()))?;
        let o = v.as_object_mut().unwrap();
        for (k, x) in p {
            if k == "id" {
                continue;
            }
            if x.is_null() {
                o.remove(k);
            } else {
                o.insert(k.clone(), x.clone());
            }
        }
        let views = self.save_views(&page, &meta, Value::Array(views)).await?;
        Ok(find_view(&views, view_id).cloned().unwrap_or_default())
    }

    pub async fn delete_view(&self, db_id: &str, who: &Principal, view_id: &str) -> Result<()> {
        let (page, meta) = self.db_for(db_id, who, true).await?;
        let mut views = meta.views.as_array().cloned().unwrap_or_default();
        let before = views.len();
        views.retain(|v| v["id"] != view_id);
        if views.len() == before {
            return Err(Error::NotFound(format!("没有视图 {view_id}")));
        }
        if views.is_empty() {
            return Err(Error::BadRequest("最后一个视图不能删".into()));
        }
        self.save_views(&page, &meta, Value::Array(views)).await?;
        Ok(())
    }
}

/// 看库的历史：只看一行、一个属性（key 或名字），翻页。
#[derive(Clone, Copy, Debug, Default)]
pub struct HistoryQuery<'a> {
    pub row: Option<&'a str>,
    pub prop: Option<&'a str>,
    pub limit: usize,
    pub cursor: Option<&'a str>,
}

/// 服务器算的查询（给 Agent、给不想加载全部数据的客户端）。给了 view_id 就从那个视图出发，别的字段覆盖它。
#[derive(Deserialize, Default, Clone, Debug, utoipa::ToSchema)]
pub struct QueryReq {
    pub view_id: Option<String>,
    /// 筛选 `{ op: "and" | "or", items: [...] }`
    #[schema(value_type = Option<Object>)]
    pub filter: Option<Value>,
    /// 排序 `[{ key, dir }]`
    #[schema(value_type = Option<Vec<Object>>)]
    pub sorts: Option<Value>,
    pub group_by: Option<String>,
    pub search: Option<String>,
    /// 每列底部的汇总：`{ 属性 key: 汇总方式 }`
    #[schema(value_type = Option<Object>)]
    pub totals: Option<Map<String, Value>>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// 查询最多一次给几行。
pub const MAX_QUERY_ROWS: usize = 10_000;

/// 一行给 Agent 看：每格的文字（名字，不是 id）。
#[derive(Serialize, Clone, Debug, utoipa::ToSchema)]
pub struct ShownRow {
    pub id: String,
    pub cells: Vec<String>,
}

/// 查询结果给人看的样子（MCP 用）：列、行、总数。
pub struct Shown {
    pub columns: Vec<String>,
    pub rows: Vec<ShownRow>,
    pub total: usize,
    pub offset: usize,
}

pub fn find_view<'a>(views: &'a Value, id: &str) -> Option<&'a Value> {
    views.as_array()?.iter().find(|v| v["id"] == id)
}

/// 查询用的视图：从 view_id 那个视图出发，给了的字段覆盖。
fn build_view(views: &Value, q: &QueryReq) -> Result<View> {
    let mut v: View = match &q.view_id {
        Some(id) => {
            let x =
                find_view(views, id).ok_or_else(|| Error::NotFound(format!("没有视图 {id}")))?;
            serde_json::from_value(x.clone())
                .map_err(|e| Error::BadRequest(format!("视图不对：{e}")))?
        }
        None => View::default(),
    };
    if let Some(f) = &q.filter {
        v.filter = Some(f.clone());
    }
    if let Some(s) = &q.sorts {
        v.sorts = serde_json::from_value(s.clone())
            .map_err(|e| Error::BadRequest(format!("sorts 不对：{e}")))?;
    }
    if q.group_by.is_some() {
        v.group_by = q.group_by.clone();
    }
    if q.search.is_some() {
        v.search = q.search.clone();
    }
    if let Some(t) = &q.totals {
        v.totals = t.clone();
    }
    v.limit = Some(q.limit.unwrap_or(MAX_QUERY_ROWS).min(MAX_QUERY_ROWS));
    v.offset = q.offset;
    Ok(v)
}

/// 一次表结构改动的说明（给人看）。tr 按语言出一句话。
fn schema_summary(
    ops: &Value,
    changes: &Value,
    tr: impl Fn(&str, &[(&str, &str)]) -> String,
) -> String {
    if ops[0]["op"] == "undo" {
        return tr(
            "撤销了第 {seq} 次改动",
            &[("seq", &ops[0]["seq"].to_string())],
        );
    }
    let parts: Vec<String> = changes
        .as_array()
        .map(|a| {
            a.iter()
                .map(|c| {
                    let name = c["name"].as_str().unwrap_or("");
                    let from = c["from"].as_str().unwrap_or("");
                    let zh = match c["op"].as_str().unwrap_or("") {
                        "add" => "加了「{name}」",
                        "delete" => "删了「{name}」",
                        "rename" => "「{from}」改名为「{name}」",
                        "change_type" => "「{name}」改了类型",
                        "set_options" => "改了「{name}」的选项",
                        "set_formula" => "改了「{name}」的公式",
                        "move" => "挪了「{name}」",
                        _ => "改了「{name}」",
                    };
                    tr(zh, &[("name", name), ("from", from)])
                })
                .collect()
        })
        .unwrap_or_default();
    if parts.is_empty() {
        match ops[0]["op"].as_str() {
            Some("add_options") => tr("加了选项", &[]),
            _ => tr("改了表结构", &[]),
        }
    } else {
        parts.join(&tr("，", &[]))
    }
}
