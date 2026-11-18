//! 联网测试，默认不跑：`cargo test -- --ignored`。网站随时会变，挂了先看是不是对方的问题。

use unfurl::{Embed, Kind, Provider, Source, Unfurler};

async fn get(url: &str) -> unfurl::Unfurl {
    Unfurler::from_env().unwrap().unfurl(url).await.unwrap_or_else(|e| panic!("{url}: {e}"))
}

#[tokio::test]
#[ignore]
async fn youtube() {
    let u = get("https://youtu.be/dQw4w9WgXcQ?t=42").await;
    assert_eq!(u.provider, Provider::Youtube);
    assert_eq!(u.kind, Kind::Video);
    assert!(u.title.is_some());
    match u.embed {
        Some(Embed::Youtube(y)) => {
            assert_eq!(y.video_id, "dQw4w9WgXcQ");
            assert_eq!(y.start_seconds, Some(42));
            assert!(y.embed_url.contains("youtube-nocookie.com"));
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
#[ignore]
async fn x_with_video() {
    let u = get("https://x.com/elonmusk/status/1585341984679469056").await;
    assert_eq!(u.provider, Provider::X);
    assert_eq!(u.source, Source::Syndication);
    let Some(Embed::X(p)) = u.embed else { panic!() };
    assert!(p.media.iter().any(|m| m.url.contains(".mp4")));
    assert!(p.like_count.unwrap_or(0) > 0);
}

#[tokio::test]
#[ignore]
async fn x_with_photo() {
    let u = get("https://x.com/TheEllenShow/status/440322224407314432").await;
    assert!(u.thumbnail.is_some());
}

#[tokio::test]
#[ignore]
async fn github_repo_issue_pr() {
    let r = get("https://github.com/rust-lang/rust").await;
    assert_eq!(r.kind, Kind::Repo);
    let i = get("https://github.com/rust-lang/rust/pull/100000").await;
    assert!(matches!(i.kind, Kind::Issue | Kind::PullRequest));
}

#[tokio::test]
#[ignore]
async fn bilibili() {
    let u = get("https://www.bilibili.com/video/BV1GJ411x7h7").await;
    assert_eq!(u.provider, Provider::Bilibili);
    assert!(u.thumbnail.unwrap().url.starts_with("https://"));
}

#[tokio::test]
#[ignore]
async fn generic_gbk() {
    let u = get("https://www.zol.com.cn/").await;
    assert!(u.title.unwrap().contains("中关村"));
}
