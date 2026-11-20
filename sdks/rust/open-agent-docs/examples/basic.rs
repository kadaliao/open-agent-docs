//! 最基本的用法：建页面、写 Markdown、读回来、翻版本、收事件。
//!
//!     OAD_BASE_URL=http://127.0.0.1:8848 OAD_TOKEN=oad_… cargo run --example basic
use futures_util::{StreamExt, TryStreamExt};
use oad::types::{CreatePage, PatchPage, SaveBody, WorkspaceEvent};
use open_agent_docs as oad;
use std::time::Duration;

#[tokio::main]
async fn main() -> oad::Result<()> {
    let client = oad::Client::from_env()?;
    let me = client.me().get().await?;
    println!("我是 {}（{}）", me.user.name, me.user.kind);
    let ws = client.default_workspace().await?;

    // 先开事件流，后面建页面时能收到 page.created
    let mut events = client.workspaces().events(&ws).await?;

    let page = client
        .pages()
        .create_with_markdown(
            &ws,
            CreatePage {
                title: "SDK 示例".into(),
                ..Default::default()
            },
            "# 本周\n\n- 写了 Rust SDK\n- 发了 0.2.0",
        )
        .await?;
    println!("建好了：{} {}", page.id, page.title);

    let content = client.pages().content().get(&page.id).await?;
    println!(
        "正文（版本 {}）：\n{}",
        content.version,
        content.markdown.unwrap_or_default()
    );

    client
        .pages()
        .update(
            &page.id,
            PatchPage {
                icon: "📝".into(),
                ..Default::default()
            },
        )
        .await?;
    client
        .pages()
        .versions()
        .create(
            &page.id,
            SaveBody {
                label: "第一版".into(),
            },
        )
        .await?;

    let versions: Vec<_> = client
        .pages()
        .versions()
        .list_all(&page.id)
        .limit(20)
        .try_collect()
        .await?;
    println!("{} 个版本", versions.len());

    let hits = client.search(&ws, "Rust").limit(5).await?;
    for h in hits {
        println!("搜到：{} — {}", h.title, h.snippet);
    }

    // 事件：等一会儿看看
    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(ev) = events.next().await {
            match ev?.data {
                WorkspaceEvent::PageCreated(p) => println!("事件：新页面 {}", p.title),
                WorkspaceEvent::PageUpdated(p) => println!("事件：页面改了 {}", p.title),
                _ => {}
            }
        }
        Ok::<_, oad::Error>(())
    })
    .await;

    // 删掉（进回收站）；再取就是 404
    client.pages().delete(&page.id).await?;
    match client.pages().get(&page.id).await {
        Err(e) if e.is_not_found() => println!("删了"),
        other => println!("{other:?}"),
    }
    Ok(())
}
