//! 链接解析服务：在 oad-unfurl 外面加缓存、图片转存和后台预取。
//!
//! - 缓存按规范化后的网址（[`oad_unfurl::cache_key`]）存表；有效期见 [`oad_unfurl::ttl`]。过期了先给旧的（`stale: true`）、
//!   后台刷新；失败按 1 分钟、10 分钟、1 小时退避（有旧数据时还用旧数据）。
//! - 同一个网址同时只抓一次（大家等同一个结果）。
//! - 卡片里的图片（封面、头像、网站图标、帖子图片、视频封面）转存到本地，地址换成 `/api/v1/media/<hash>`；视频文件不转存。
//! - 上传的文件网址直接从库里给 kind、文件名、大小、类型、宽高、blurhash、时长。
//! - 有人写了单独一行网址（embed 块）时，同步中心把网址交过来，攒一小会儿后在后台解析。
use crate::ids::now_ms;
use crate::media::{Media, file_url, own_file_id};
use crate::model::*;
use crate::storage::Storage;
use crate::{Error, Result};
use futures_util::future::{BoxFuture, FutureExt, Shared};
use oad_unfurl::{Unfurler, cache_key, failure_backoff, ttl};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::debug;

/// 解析时最多等图片转存多久。
const REHOST_WAIT: Duration = Duration::from_millis(1500);

type Flight = Shared<BoxFuture<'static, std::result::Result<Value, String>>>;

pub struct UnfurlService {
    storage: Arc<dyn Storage>,
    media: Arc<Media>,
    unfurler: Unfurler,
    inflight: Mutex<HashMap<String, Flight>>,
    prefetch: mpsc::UnboundedSender<String>,
}

impl UnfurlService {
    pub fn new(
        storage: Arc<dyn Storage>,
        media: Arc<Media>,
        unfurler: Unfurler,
    ) -> Arc<UnfurlService> {
        let (tx, rx) = mpsc::unbounded_channel();
        let s = Arc::new(UnfurlService {
            storage,
            media,
            unfurler,
            inflight: Mutex::new(HashMap::new()),
            prefetch: tx,
        });
        tokio::spawn(prefetcher(Arc::downgrade(&s), rx));
        s
    }

    /// 后台预取（攒一小会儿、去重后解析，已经在缓存里的不再抓）。
    pub fn prefetch_sink(&self) -> mpsc::UnboundedSender<String> {
        self.prefetch.clone()
    }

    /// 解析一个网址。上传的文件由调用方先查权限（见 [`crate::Core::unfurl`]）。
    pub async fn get(self: &Arc<Self>, url: &str) -> Result<Value> {
        let url = url.trim();
        if url.is_empty() || url.len() > 4096 {
            return Err(Error::BadRequest("网址不对".into()));
        }
        let key = cache_key(url);
        let now = now_ms();
        match self.storage.unfurl(&key).await? {
            Some(row) if row.ok && row.data.is_some() => {
                let mut v = row.data.unwrap();
                if now >= row.expires_at {
                    // 先给旧的，后台刷新
                    let me = self.clone();
                    let (k, u) = (key.clone(), url.to_string());
                    tokio::spawn(async move {
                        let _ = me.fetch(k, u).await;
                    });
                    v["stale"] = true.into();
                }
                Ok(v)
            }
            Some(row) if !row.ok && now < row.expires_at => Err(Error::BadGateway(
                row.error.unwrap_or_else(|| "解析失败".into()),
            )),
            _ => self
                .fetch(key, url.to_string())
                .await
                .map_err(Error::BadGateway),
        }
    }

    /// 抓一次（同一个键同时只有一个在抓）。
    async fn fetch(
        self: &Arc<Self>,
        key: String,
        url: String,
    ) -> std::result::Result<Value, String> {
        let flight = {
            let mut m = self.inflight.lock().unwrap();
            match m.get(&key) {
                Some(f) => f.clone(),
                None => {
                    let me = self.clone();
                    let k = key.clone();
                    let f = async move {
                        let r = me.fetch_and_store(&k, &url).await;
                        me.inflight.lock().unwrap().remove(&k);
                        r
                    }
                    .boxed()
                    .shared();
                    m.insert(key.clone(), f.clone());
                    f
                }
            }
        };
        flight.await
    }

    async fn fetch_and_store(
        self: &Arc<Self>,
        key: &str,
        url: &str,
    ) -> std::result::Result<Value, String> {
        let prev = self.storage.unfurl(key).await.ok().flatten();
        let now = now_ms();
        match self.unfurler.unfurl(url).await {
            Ok(u) => {
                let expires = now + ttl(&u).as_millis() as i64;
                let mut v = serde_json::to_value(&u).map_err(|e| e.to_string())?;
                if let Some(o) = v.as_object_mut() {
                    o.remove("attempts");
                }
                let row = |data: Value| UnfurlRow {
                    url_key: key.into(),
                    data: Some(data),
                    ok: true,
                    fetched_at: now,
                    expires_at: expires,
                    error: None,
                    failures: 0,
                };
                // 转存图片最多等 REHOST_WAIT；没转完先给原地址，转完再更新缓存（下次就是本地地址）
                let me = self.clone();
                let mut job = {
                    let v = v.clone();
                    tokio::spawn(async move {
                        let mut v = v;
                        me.rehost_images(&mut v).await;
                        v
                    })
                };
                match tokio::time::timeout(REHOST_WAIT, &mut job).await {
                    Ok(Ok(done)) => {
                        v = done;
                        let _ = self.storage.put_unfurl(row(v.clone())).await;
                    }
                    _ => {
                        // 先存原地址的，转完再覆盖（顺序不能反）
                        let _ = self.storage.put_unfurl(row(v.clone())).await;
                        let (me, done_row) = (self.clone(), row(Value::Null));
                        tokio::spawn(async move {
                            if let Ok(done) = job.await {
                                let _ = me
                                    .storage
                                    .put_unfurl(UnfurlRow {
                                        data: Some(done),
                                        ..done_row
                                    })
                                    .await;
                            }
                        });
                    }
                }
                Ok(v)
            }
            Err(e) => {
                let msg = e.to_string();
                debug!(url, "解析失败：{msg}");
                let failures = prev.as_ref().map_or(0, |p| p.failures) + 1;
                let retry = now + failure_backoff(failures as u32).as_millis() as i64;
                // 有旧的好数据就继续用旧的，过一会儿再试
                let keep = prev.as_ref().is_some_and(|p| p.ok && p.data.is_some());
                let _ = self
                    .storage
                    .put_unfurl(UnfurlRow {
                        url_key: key.into(),
                        data: None,
                        ok: keep,
                        fetched_at: now,
                        expires_at: retry,
                        error: Some(msg.clone()),
                        failures,
                    })
                    .await;
                match prev {
                    Some(p) if keep => Ok(p.data.unwrap()),
                    _ => Err(msg),
                }
            }
        }
    }

    /// 把卡片里的图片换成本地地址（并发下载，失败的保留原地址）。
    async fn rehost_images(&self, v: &mut Value) {
        let mut urls = HashSet::new();
        walk_images(v, &mut |s: &mut String| {
            urls.insert(s.clone());
        });
        let urls: Vec<String> = urls.into_iter().filter(|u| u.starts_with("http")).collect();
        let got: HashMap<String, (String, BlobInfo)> = futures_util::stream::iter(urls)
            .map(|u| async move { self.media.rehost(&u).await.map(|r| (u, r)) })
            .buffer_unordered(4)
            .filter_map(|x| async move { x })
            .collect()
            .await;
        if got.is_empty() {
            return;
        }
        // 封面带上宽高和 blurhash（网页先画模糊占位）
        if let Some(t) = v.get_mut("thumbnail").and_then(Value::as_object_mut)
            && let Some((_, b)) = t
                .get("url")
                .and_then(Value::as_str)
                .and_then(|u| got.get(u))
        {
            t.entry("width").or_insert(b.width.into());
            t.entry("height").or_insert(b.height.into());
            if let Some(h) = &b.blurhash {
                t.insert("blurhash".into(), h.clone().into());
            }
        }
        walk_images(v, &mut |s: &mut String| {
            if let Some((local, _)) = got.get(s.as_str()) {
                *s = local.clone();
            }
        });
    }
}

use futures_util::StreamExt;

/// 卡片里的每个图片地址：`favicon`、`avatar`、`account_avatar`、`poster`、`image`（字符串）、`thumbnail.url`、
/// `media[]` 里图片的 `url`。视频、GIF 的 `url` 是视频文件，不算。
fn walk_images(v: &mut Value, f: &mut dyn FnMut(&mut String)) {
    match v {
        Value::Object(o) => {
            for (k, x) in o.iter_mut() {
                match (k.as_str(), x) {
                    (
                        "favicon" | "avatar" | "account_avatar" | "poster" | "image",
                        Value::String(s),
                    ) => f(s),
                    ("thumbnail", Value::Object(t)) => {
                        if let Some(Value::String(s)) = t.get_mut("url") {
                            f(s);
                        }
                    }
                    ("media", Value::Array(items)) => {
                        for it in items {
                            let image = it
                                .get("kind")
                                .or_else(|| it.get("type"))
                                .and_then(Value::as_str)
                                .is_some_and(|k| k == "image" || k == "photo");
                            if image && let Some(Value::String(s)) = it.get_mut("url") {
                                f(s);
                            }
                            walk_images(it, f);
                        }
                    }
                    (_, x) => walk_images(x, f),
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| walk_images(x, f)),
        _ => {}
    }
}

/// 预取：收到网址后等到 300ms 没有新的，再一个个解析（缓存里有、没过期的不抓）。
async fn prefetcher(svc: std::sync::Weak<UnfurlService>, mut rx: mpsc::UnboundedReceiver<String>) {
    let mut batch: HashSet<String> = HashSet::new();
    loop {
        let next = if batch.is_empty() {
            rx.recv().await
        } else {
            match tokio::time::timeout(Duration::from_millis(300), rx.recv()).await {
                Ok(x) => x,
                Err(_) => {
                    let Some(s) = svc.upgrade() else { return };
                    for url in batch.drain() {
                        if own_file_id(&url).is_some() {
                            continue;
                        }
                        let s = s.clone();
                        tokio::spawn(async move {
                            if let Err(e) = s.get(&url).await {
                                debug!(url, "预取失败：{e}");
                            }
                        });
                    }
                    continue;
                }
            }
        };
        match next {
            Some(u) => {
                batch.insert(u);
            }
            None => return,
        }
    }
}

impl crate::Core {
    /// 解析一个网址（GET /unfurl）。上传的文件要能看那个工作区，直接从库里给。
    pub async fn unfurl(&self, url: &str, who: &Principal) -> Result<Value> {
        if let Some(id) = own_file_id(url) {
            let f = self
                .storage
                .file(&id)
                .await?
                .ok_or_else(|| Error::NotFound("文件不存在".into()))?;
            // 不是那个工作区的成员：当不存在
            if self.require(&f.workspace_id, who, false).await.is_err() {
                return Err(Error::NotFound("文件不存在".into()));
            }
            return Ok(file_card(&f));
        }
        match &self.unfurls {
            Some(u) => u.get(url).await,
            None => Err(Error::NotFound("没有开链接解析".into())),
        }
    }
}

/// 上传的文件当卡片：kind 是 image / file，带文件名、类型、大小、宽高、blurhash、时长。
pub fn file_card(f: &FileRow) -> Value {
    let b = &f.blob;
    let url = file_url(&f.id, &f.name);
    let image = b.mime.starts_with("image/");
    let mut v = json!({
        "kind": if image { "image" } else { "file" },
        "provider": "upload",
        "source": "api",
        "url": url,
        "title": f.name,
        "name": f.name,
        "mime": b.mime,
        "size": b.size,
    });
    let o = v.as_object_mut().unwrap();
    for (k, x) in [
        ("width", b.width),
        ("height", b.height),
        ("duration_ms", b.duration_ms),
    ] {
        if let Some(x) = x {
            o.insert(k.into(), x.into());
        }
    }
    if let Some(h) = &b.blurhash {
        o.insert("blurhash".into(), h.clone().into());
    }
    if image {
        o.insert(
            "thumbnail".into(),
            json!({ "url": url, "width": b.width, "height": b.height, "blurhash": b.blurhash }),
        );
    }
    v
}
