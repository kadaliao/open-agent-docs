//! Postgres 存储：表结构升级和几样核心操作。要设 OAD_TEST_POSTGRES_URL 才跑（没设就跳过），
//! 每个测试用自己的 schema，跑完删掉。
use oad_core::model::*;
use oad_core::storage::pg::Postgres;
use oad_core::{Error, Storage};
use std::sync::Arc;

fn url() -> Option<String> {
    std::env::var("OAD_TEST_POSTGRES_URL")
        .ok()
        .filter(|v| !v.is_empty())
}

/// 一个临时 schema：drop 时删掉。
struct Schema {
    url: String,
    name: String,
}

impl Schema {
    fn new(url: &str) -> Schema {
        let n: u64 = rand_u64();
        Schema {
            url: url.into(),
            name: format!("oad_core_t_{n:016x}"),
        }
    }

    async fn sql(&self, sql: String) -> Vec<postgres::Row> {
        let url = self.url.clone();
        tokio::task::spawn_blocking(move || {
            let mut c = postgres::Client::connect(&url, postgres::NoTls).unwrap();
            c.query(&sql, &[]).unwrap()
        })
        .await
        .unwrap()
    }

    async fn open(&self) -> Arc<Postgres> {
        Arc::new(
            Postgres::connect_with(&self.url, Some(&self.name), 4)
                .await
                .unwrap(),
        )
    }
}

impl Drop for Schema {
    fn drop(&mut self) {
        let (url, name) = (self.url.clone(), self.name.clone());
        // 不在 tokio 线程上用同步驱动
        std::thread::spawn(move || {
            if let Ok(mut c) = postgres::Client::connect(&url, postgres::NoTls) {
                let _ = c.batch_execute(&format!("DROP SCHEMA IF EXISTS {name} CASCADE"));
            }
        })
        .join()
        .unwrap();
    }
}

fn rand_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    );
    h.finish()
}

fn new_page(ws: &str, parent: Option<&str>, title: &str) -> NewPage {
    NewPage {
        workspace_id: ws.into(),
        parent_id: parent.map(str::to_string),
        place: Placement::Last,
        title: title.into(),
        icon: None,
        created_by: None,
        initial_state: vec![1, 2, 3],
        kind: "page".into(),
    }
}

macro_rules! need_pg {
    () => {
        match url() {
            Some(u) => u,
            None => {
                eprintln!("没设 OAD_TEST_POSTGRES_URL，跳过");
                return;
            }
        }
    };
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn migrate_users_pages() {
    let url = need_pg!();
    let sc = Schema::new(&url);
    let s = sc.open().await;
    // 再连一次：表结构不重复建
    drop(sc.open().await);
    let v = sc
        .sql(format!(
            "SELECT max(version) FROM {}.oad_migrations",
            sc.name
        ))
        .await;
    assert_eq!(v[0].get::<_, i64>(0), 5);

    // 邮箱不分大小写
    let u = s
        .create_user(NewUser {
            name: "A".into(),
            email: Some("Ann@Example.com".into()),
            password_hash: Some("h".into()),
            external_id: None,
            kind: PrincipalKind::User,
        })
        .await
        .unwrap();
    let (found, hash) = s.user_by_email("ann@example.COM").await.unwrap().unwrap();
    assert_eq!(
        (found.id.as_str(), hash.as_deref()),
        (u.id.as_str(), Some("h"))
    );
    let dup = s
        .create_user(NewUser {
            name: "B".into(),
            email: Some("ANN@example.com".into()),
            password_hash: None,
            external_id: None,
            kind: PrincipalKind::User,
        })
        .await;
    assert!(matches!(dup, Err(Error::Conflict(_))), "{dup:?}");
    assert_eq!(s.count_users().await.unwrap(), 1);

    // 页面树：位置按字节排序，移动、删除、恢复
    let ws = s.create_workspace("w").await.unwrap();
    s.set_member(&ws.id, &u.id, Role::Owner).await.unwrap();
    assert_eq!(
        s.member_role(&ws.id, &u.id).await.unwrap(),
        Some(Role::Owner)
    );
    let a = s.create_page(new_page(&ws.id, None, "a")).await.unwrap();
    let b = s.create_page(new_page(&ws.id, None, "b")).await.unwrap();
    let c = s
        .create_page(new_page(&ws.id, Some(&a.id), "c"))
        .await
        .unwrap();
    let pages = s.pages(&ws.id).await.unwrap();
    assert_eq!(pages.len(), 3);
    assert!(pages[0].parent_id.is_none() && pages[1].parent_id.is_none());
    assert_eq!(
        (pages[0].title.as_str(), pages[1].title.as_str()),
        ("a", "b")
    );
    let moved = s.move_page(&b.id, None, Placement::First).await.unwrap();
    assert!(moved.position < a.position);
    assert!(
        s.move_page(&a.id, Some(c.id.clone()), Placement::Last)
            .await
            .is_err(),
        "不能移到自己的子页下面"
    );
    let gone = s.delete_page(&a.id).await.unwrap();
    assert_eq!(gone.len(), 2);
    assert_eq!(s.trash(&ws.id).await.unwrap().len(), 1);
    assert_eq!(s.ancestors(&c.id).await.unwrap(), vec![a.id.clone()]);
    let back = s.restore_page(&a.id).await.unwrap();
    assert_eq!(back.len(), 2);

    // 正文：快照 + 增量，压缩
    s.append_update(&c.id, vec![9, 9]).await.unwrap();
    s.append_update(&c.id, vec![8]).await.unwrap();
    let log = s.load_doc(&c.id).await.unwrap();
    assert_eq!(log.snapshot, Some(vec![1, 2, 3]));
    assert_eq!(log.updates, vec![vec![9, 9], vec![8]]);
    s.compact(&c.id, vec![7]).await.unwrap();
    let log = s.load_doc(&c.id).await.unwrap();
    assert_eq!((log.snapshot, log.updates.len()), (Some(vec![7]), 0));

    // 分享：布尔列
    let sh = s.put_share(&c.id, true, false, &u.id).await.unwrap();
    assert!(sh.include_children && !sh.indexable);
    let sh2 = s.put_share(&c.id, false, true, &u.id).await.unwrap();
    assert_eq!(sh2.token, sh.token);
    assert!(!sh2.include_children && sh2.indexable);

    // 彻底删除
    s.delete_page(&a.id).await.unwrap();
    let (ids, _) = s.purge_page(&a.id).await.unwrap();
    assert_eq!(ids.len(), 2);
    assert!(s.page(&c.id).await.unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_writes_and_notes() {
    let url = need_pg!();
    let sc = Schema::new(&url);
    let s = sc.open().await;
    let ws = s.create_workspace("w").await.unwrap();
    // 同一个父页面下同时建：写事务排队，位置不会撞
    let mut hs = Vec::new();
    for i in 0..24 {
        let (s, ws) = (s.clone(), ws.id.clone());
        hs.push(tokio::spawn(async move {
            s.create_page(new_page(&ws, None, &format!("p{i}")))
                .await
                .unwrap()
        }));
    }
    for h in hs {
        h.await.unwrap();
    }
    let pages = s.pages(&ws.id).await.unwrap();
    let mut pos: Vec<_> = pages.iter().map(|p| p.position.clone()).collect();
    pos.dedup();
    assert_eq!(pos.len(), 24);

    // Agent 和通知：自增编号、游标
    let ag = s
        .create_agent(NewAgent {
            workspace_id: ws.id.clone(),
            name: "bot".into(),
            about: String::new(),
            scope: Scope::Write,
            webhook: None,
            webhook_secret: "x".into(),
            created_by: None,
            token_hash: "t".into(),
        })
        .await
        .unwrap();
    assert_eq!(s.agent_by_token("t").await.unwrap().unwrap().id, ag.id);
    let rows: Vec<NewNotification> = (0..3)
        .map(|i| NewNotification {
            workspace_id: ws.id.clone(),
            owner_id: ag.id.clone(),
            kind: "mention".into(),
            data: serde_json::json!({ "i": i }),
            next_attempt_at: None,
        })
        .collect();
    let added = s.add_agent_notifications(rows).await.unwrap();
    assert!(added[0].id < added[1].id && added[1].id < added[2].id);
    let scope = NoteScope::Agent(ag.id.clone());
    assert_eq!(
        s.agent_notifications(scope.clone(), None, 10)
            .await
            .unwrap()
            .len(),
        3
    );
    let after = s
        .agent_notifications(scope.clone(), Some(added[0].id), 10)
        .await
        .unwrap();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0].data["i"], 1);
    assert_eq!(
        s.ack_agent_notifications(scope.clone(), vec![], Some(added[1].id), 1)
            .await
            .unwrap(),
        2
    );
    assert_eq!(s.agent_stats(&ws.id).await.unwrap()[0].1, 1);
    assert_eq!(s.next_webhook_at().await.unwrap(), None);
    let dup = s
        .create_agent(NewAgent {
            workspace_id: ws.id.clone(),
            name: "bot".into(),
            about: String::new(),
            scope: Scope::Read,
            webhook: None,
            webhook_secret: "x".into(),
            created_by: None,
            token_hash: "t2".into(),
        })
        .await;
    assert!(matches!(dup, Err(Error::Conflict(_))));
    // 出错的写事务整个回滚：users 里没有多出一行
    assert_eq!(s.members(&ws.id).await.unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn databases() {
    let url = need_pg!();
    let sc = Schema::new(&url);
    let s = sc.open().await;
    let ws = s.create_workspace("w").await.unwrap();
    let mut np = new_page(&ws.id, None, "db");
    np.kind = "database".into();
    let db = s
        .create_database(
            np,
            serde_json::json!({ "props": [{ "key": "t", "type": "title" }] }),
            serde_json::json!([]),
        )
        .await
        .unwrap();
    let row = |t: &str| NewRow {
        props: serde_json::Map::from_iter([("t".to_string(), serde_json::Value::from(t))]),
        after: None,
        title: t.into(),
        initial_state: vec![],
    };
    let (ids, seq) = s
        .add_db_rows(&db.id, vec![row("r1"), row("r2"), row("r3")], "me")
        .await
        .unwrap();
    assert_eq!(seq, 1);
    let mut set = serde_json::Map::new();
    set.insert("t".into(), "新标题".into());
    set.insert("n".into(), 5.into());
    let (seq2, ch) = s
        .patch_db_rows(
            &db.id,
            "t",
            vec![CellOp {
                row_id: ids[1].clone(),
                set,
                after: Some(None),
            }],
            "me",
        )
        .await
        .unwrap();
    assert_eq!(seq2, 2);
    assert_eq!(ch.len(), 3, "{ch:?}");
    assert_eq!(s.page(&ids[1]).await.unwrap().unwrap().title, "新标题");
    let (cur, rows, deleted) = s.db_rows(&db.id, None).await.unwrap();
    assert_eq!(cur, 2);
    assert_eq!(rows[0].id, ids[1], "挪到了最前面");
    assert!(deleted.is_empty());
    let (_, changed, _) = s.db_rows(&db.id, Some(1)).await.unwrap();
    assert_eq!(changed.len(), 1);
    let ops = s.db_ops(&db.id, None, None, 50, None).await.unwrap();
    assert_eq!(ops.len(), 6);
    let (seq3, titles, _) = s.undo_db_ops(&db.id, 2, "t", "me").await.unwrap();
    assert_eq!(seq3, 3);
    assert_eq!(titles, vec![(ids[1].clone(), "r2".to_string())]);
    let seq4 = s
        .delete_db_rows(&db.id, vec![ids[0].clone()], "me")
        .await
        .unwrap();
    let (_, _, deleted) = s.db_rows(&db.id, Some(seq4 - 1)).await.unwrap();
    assert_eq!(deleted, vec![ids[0].clone()]);
    let v = s
        .add_version(
            &db.id,
            NewVersion {
                snapshot: vec![1, 0, 255],
                state_vector: vec![0],
                label: Some("手动".into()),
                kind: "manual".into(),
                authors: vec![],
                created_by: None,
            },
        )
        .await
        .unwrap();
    let (meta, snap) = s.version(&db.id, v).await.unwrap().unwrap();
    assert_eq!(
        (meta.label.as_deref(), snap),
        (Some("手动"), vec![1, 0, 255])
    );
    assert_eq!(s.versions(&db.id, 10, None).await.unwrap().len(), 1);
    assert!(s.versions(&db.id, 10, Some(v)).await.unwrap().is_empty());
}

/// sslmode=require：服务器不支持 TLS 时报错（不悄悄退回不加密）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn require_tls_is_enforced() {
    let Ok(url) = std::env::var("OAD_TEST_POSTGRES_URL") else {
        return;
    };
    let sep = if url.contains('?') { '&' } else { '?' };
    let r = oad_core::storage::pg::Postgres::connect(&format!("{url}{sep}sslmode=require")).await;
    assert!(r.is_err(), "测试用的库没开 TLS，应该连不上");
}
