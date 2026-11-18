//! 文件和图片：上传、按内容判断类型、图片的宽高和 blurhash、视频音频的时长、webp 缩略图，以及外部图片的转存。
//!
//! 内容按 SHA-256 存进 [`BlobStore`]（同样的文件只存一份）；`blobs` 表记类型、宽高等，`files` 表记谁在哪个工作区传的、叫什么，
//! `media` 表记转存的外部图片原来的网址。
use crate::blob::BlobStore;
use crate::ids::{new_id, now_ms};
use crate::model::*;
use crate::storage::Storage;
use crate::{Error, Result};
use oad_unfurl::{Fetcher, RequestOpts};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// 缩略图的宽度（取最近的一档）。
pub const THUMB_WIDTHS: &[u32] = &[160, 320, 640, 960, 1280, 1920];
/// 转存的外部图片最大多少字节。
const MAX_REMOTE_IMAGE: usize = 10 << 20;

pub struct Media {
    storage: Arc<dyn Storage>,
    pub blobs: Arc<dyn BlobStore>,
    fetcher: Fetcher,
    pub max_upload: usize,
}

/// 上传接口的返回。
#[derive(serde::Serialize, Clone, Debug)]
pub struct Uploaded {
    pub id: String,
    pub url: String,
    pub name: String,
    pub mime: String,
    pub size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blurhash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
}

/// 文件的网址：`/api/v1/files/<id>/<文件名>`（文件名按网址规则编码）。
pub fn file_url(id: &str, name: &str) -> String {
    let enc: String = name
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!("/api/v1/files/{id}/{enc}")
}

pub fn media_url(hash: &str) -> String {
    format!("/api/v1/media/{hash}")
}

/// 网址是不是本服务器上传的文件（`/api/v1/files/<id>/…`，相对的或带域名的），是的话给文件编号。
pub fn own_file_id(url: &str) -> Option<String> {
    let path = if url.starts_with('/') {
        url.split(['?', '#']).next()?.to_string()
    } else {
        let u = url::Url::parse(url).ok()?;
        if !matches!(u.scheme(), "http" | "https") {
            return None;
        }
        u.path().to_string()
    };
    let id = path.strip_prefix("/api/v1/files/")?.split('/').next()?;
    (id.len() == 12 && id.bytes().all(|b| b.is_ascii_alphanumeric())).then(|| id.to_string())
}

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 按内容判断类型，不信客户端给的。HTML 一律当纯文本（不能让浏览器当网页打开）。
pub fn sniff(data: &[u8], name: &str) -> String {
    if let Some(k) = infer::get(data) {
        let m = k.mime_type();
        if !(m.starts_with("text/html") || m.contains("xhtml")) {
            return m.to_string();
        }
    }
    let head = String::from_utf8_lossy(&data[..data.len().min(2048)]).to_ascii_lowercase();
    let head = head.trim_start_matches('\u{feff}').trim_start();
    if head.starts_with("<svg") || (head.starts_with("<?xml") && head.contains("<svg")) {
        return "image/svg+xml".into();
    }
    let sample = &data[..data.len().min(8192)];
    let text = match std::str::from_utf8(sample) {
        Ok(_) => true,
        // 截断处可能切在多字节字符中间
        Err(e) => e.valid_up_to() + 4 >= sample.len(),
    };
    if !text {
        return "application/octet-stream".into();
    }
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "md" | "markdown" => "text/markdown",
        "csv" => "text/csv",
        "json" => "application/json",
        _ => "text/plain",
    }
    .into()
}

/// 能做缩略图、算 blurhash 的图片（SVG、GIF 动图不算）。
fn raster(mime: &str) -> bool {
    matches!(
        mime,
        "image/jpeg" | "image/png" | "image/webp" | "image/bmp"
    )
}

/// 图片的宽高和 blurhash。
fn image_meta(data: &[u8], mime: &str) -> (Option<i64>, Option<i64>, Option<String>) {
    let Ok(reader) = image::ImageReader::new(std::io::Cursor::new(data)).with_guessed_format()
    else {
        return (None, None, None);
    };
    let Ok((w, h)) = reader.into_dimensions() else {
        return (None, None, None);
    };
    let hash = if raster(mime) || mime == "image/gif" {
        image::load_from_memory(data).ok().and_then(|img| {
            let small = img.thumbnail(32, 32).to_rgba8();
            blurhash::encode(4, 3, small.width(), small.height(), small.as_raw()).ok()
        })
    } else {
        None
    };
    (Some(w as i64), Some(h as i64), hash)
}

/// MP4 / MOV / M4A 的时长和画面大小：读 moov 里的 mvhd、第一个有画面的 tkhd。
pub fn mp4_meta(data: &[u8]) -> (Option<i64>, Option<i64>, Option<i64>) {
    fn boxes(d: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
        let mut i = 0usize;
        std::iter::from_fn(move || {
            if i + 8 > d.len() {
                return None;
            }
            let mut size = u32::from_be_bytes(d[i..i + 4].try_into().ok()?) as usize;
            let ty = &d[i + 4..i + 8];
            let mut hdr = 8;
            if size == 1 {
                size = u64::from_be_bytes(d.get(i + 8..i + 16)?.try_into().ok()?) as usize;
                hdr = 16;
            } else if size == 0 {
                size = d.len() - i;
            }
            if size < hdr || i + size > d.len() {
                return None;
            }
            let body = &d[i + hdr..i + size];
            i += size;
            Some((ty, body))
        })
    }
    let be32 = |b: &[u8], at: usize| {
        b.get(at..at + 4)
            .map(|x| u32::from_be_bytes(x.try_into().unwrap()) as u64)
    };
    let be64 = |b: &[u8], at: usize| {
        b.get(at..at + 8)
            .map(|x| u64::from_be_bytes(x.try_into().unwrap()))
    };
    let Some((_, moov)) = boxes(data).find(|(t, _)| *t == b"moov") else {
        return (None, None, None);
    };
    let mut dur = None;
    let (mut w, mut h) = (None, None);
    for (t, b) in boxes(moov) {
        if t == b"mvhd" {
            let v = b.first().copied().unwrap_or(0);
            let (scale, d) = if v == 1 {
                (be32(b, 20), be64(b, 24))
            } else {
                (be32(b, 12), be32(b, 16))
            };
            if let (Some(s), Some(d)) = (scale, d)
                && s > 0
            {
                dur = Some((d * 1000 / s) as i64);
            }
        } else if t == b"trak" && w.is_none() {
            for (t2, b2) in boxes(b) {
                if t2 == b"tkhd" {
                    let v = b2.first().copied().unwrap_or(0);
                    let at = if v == 1 { 88 } else { 76 };
                    let (tw, th) = (
                        be32(b2, at).unwrap_or(0) >> 16,
                        be32(b2, at + 4).unwrap_or(0) >> 16,
                    );
                    if tw > 0 && th > 0 {
                        w = Some(tw as i64);
                        h = Some(th as i64);
                    }
                }
            }
        }
    }
    (dur, w, h)
}

/// 一份内容的信息（类型、宽高、blurhash、时长）。
pub fn describe(data: &[u8], name: &str) -> BlobInfo {
    let mime = sniff(data, name);
    let mut b = BlobInfo {
        hash: sha256_hex(data),
        size: data.len() as i64,
        mime: mime.clone(),
        width: None,
        height: None,
        blurhash: None,
        duration_ms: None,
    };
    if mime.starts_with("image/") && mime != "image/svg+xml" {
        (b.width, b.height, b.blurhash) = image_meta(data, &mime);
    } else if matches!(
        mime.as_str(),
        "video/mp4" | "video/quicktime" | "audio/m4a" | "audio/mp4" | "video/x-m4v"
    ) {
        (b.duration_ms, b.width, b.height) = mp4_meta(data);
    }
    b
}

impl Media {
    pub fn new(
        storage: Arc<dyn Storage>,
        blobs: Arc<dyn BlobStore>,
        fetcher: Fetcher,
        max_upload: usize,
    ) -> Media {
        Media {
            storage,
            blobs,
            fetcher,
            max_upload,
        }
    }

    /// 存一份内容（已经有了只记信息），返回它的信息。
    pub async fn store(&self, data: Vec<u8>, name: &str) -> Result<BlobInfo> {
        let name = name.to_string();
        let (data, info) = tokio::task::spawn_blocking(move || {
            let info = describe(&data, &name);
            (data, info)
        })
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
        if let Some(b) = self.storage.blob(&info.hash).await? {
            return Ok(b);
        }
        self.blobs.put(&info.hash, data).await?;
        self.storage.put_blob(info.clone()).await?;
        Ok(info)
    }

    /// 上传：记成工作区里的一个文件。调用方先查好权限。
    pub async fn upload(
        &self,
        workspace_id: &str,
        who: &Principal,
        name: &str,
        data: Vec<u8>,
    ) -> Result<Uploaded> {
        if data.len() > self.max_upload {
            return Err(Error::BadRequest(format!(
                "文件太大（最多 {} MB）",
                self.max_upload >> 20
            )));
        }
        if data.is_empty() {
            return Err(Error::BadRequest("文件是空的".into()));
        }
        let name = clean_name(name);
        let blob = self.store(data, &name).await?;
        let id = new_id();
        self.storage
            .create_file(FileRow {
                id: id.clone(),
                workspace_id: workspace_id.into(),
                name: name.clone(),
                created_by: Some(who.user_id.clone()),
                created_at: now_ms(),
                blob: blob.clone(),
            })
            .await?;
        Ok(Uploaded {
            url: file_url(&id, &name),
            id,
            name,
            mime: blob.mime,
            size: blob.size,
            width: blob.width,
            height: blob.height,
            blurhash: blob.blurhash,
            duration_ms: blob.duration_ms,
        })
    }

    /// 内容（整份或一段）。
    pub async fn read(&self, hash: &str, range: Option<(u64, u64)>) -> Result<Vec<u8>> {
        let d = match range {
            Some((a, b)) => self.blobs.range(hash, a, b).await?,
            None => self.blobs.get(hash).await?,
        };
        d.ok_or_else(|| Error::NotFound("内容不见了".into()))
    }

    /// 图片的 webp 缩略图（宽度取最近的一档，生成后缓存）。不是能缩的图片、或者本来就不比这宽时返回 None（直接给原图）。
    pub async fn thumbnail(&self, b: &BlobInfo, want: u32) -> Result<Option<Vec<u8>>> {
        if !raster(&b.mime) {
            return Ok(None);
        }
        let w = *THUMB_WIDTHS
            .iter()
            .min_by_key(|x| (**x as i64 - want as i64).abs())
            .unwrap();
        if b.width.is_some_and(|ow| ow <= w as i64) {
            return Ok(None);
        }
        let key = format!("{}.w{w}.webp", b.hash);
        if let Some(d) = self.blobs.get(&key).await? {
            return Ok(Some(d));
        }
        let src = self.read(&b.hash, None).await?;
        let out = tokio::task::spawn_blocking(move || -> Option<Vec<u8>> {
            let img = image::load_from_memory(&src).ok()?;
            if img.width() <= w {
                return None;
            }
            let h = ((img.height() as u64 * w as u64) / img.width() as u64).max(1) as u32;
            let small = img
                .resize_exact(w, h, image::imageops::FilterType::Triangle)
                .to_rgba8();
            let enc = webp::Encoder::from_rgba(small.as_raw(), w, h);
            Some(enc.encode(80.0).to_vec())
        })
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
        let Some(out) = out else { return Ok(None) };
        self.blobs.put(&key, out.clone()).await?;
        Ok(Some(out))
    }

    /// 把外部图片下载下来存好，返回本地地址；不是图片、下载失败返回 None（调用方保留原地址）。
    pub async fn rehost(&self, url: &str) -> Option<(String, BlobInfo)> {
        if let Ok(Some(b)) = self.storage.media_by_source(url).await {
            return Some((media_url(&b.hash), b));
        }
        let u = url::Url::parse(url).ok()?;
        let opts = RequestOpts {
            accept: Some("image/avif,image/webp,image/png,image/svg+xml,image/*;q=0.8,*/*;q=0.5"),
            max_body: Some(MAX_REMOTE_IMAGE),
            ..RequestOpts::default()
        };
        let got = self.fetcher.request(&u, opts).await.ok()?.ok().ok()?;
        if got.truncated || got.body.is_empty() {
            return None;
        }
        let b = self.store(got.body, "").await.ok()?;
        if !b.mime.starts_with("image/") {
            return None;
        }
        self.storage.put_media(&b.hash, url).await.ok()?;
        Some((media_url(&b.hash), b))
    }
}

/// 文件名去掉路径和控制字符，太长的截断；空的叫 file。
fn clean_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let n: String = base
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '"' | '<' | '>'))
        .take(200)
        .collect();
    let n = n.trim().trim_start_matches('.').to_string();
    if n.is_empty() { "file".into() } else { n }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffing() {
        let png = {
            let mut v = Vec::new();
            image::RgbImage::new(4, 3)
                .write_to(&mut std::io::Cursor::new(&mut v), image::ImageFormat::Png)
                .unwrap();
            v
        };
        assert_eq!(sniff(&png, "a.txt"), "image/png", "不信文件名");
        assert_eq!(
            sniff(b"<svg xmlns='http://www.w3.org/2000/svg'/>", "a.png"),
            "image/svg+xml"
        );
        assert_eq!(
            sniff(b"<!doctype html><script>alert(1)</script>", "a.html"),
            "text/plain"
        );
        assert_eq!(sniff("# 标题\n".as_bytes(), "a.md"), "text/markdown");
        assert_eq!(
            sniff(&[0u8, 159, 146, 150, 1, 2, 3], "x"),
            "application/octet-stream"
        );
        let d = describe(&png, "a.png");
        assert_eq!((d.width, d.height), (Some(4), Some(3)));
        assert!(d.blurhash.is_some());
        assert_eq!(clean_name("../../etc/passwd"), "passwd");
        assert_eq!(clean_name(".hidden"), "hidden");
        assert_eq!(
            own_file_id("/api/v1/files/abcdefABCDEF/x.png"),
            Some("abcdefABCDEF".into())
        );
        assert_eq!(
            own_file_id("https://docs.example.com/api/v1/files/abcdefABCDEF/x.png?w=1"),
            Some("abcdefABCDEF".into())
        );
        assert_eq!(
            own_file_id("https://evil.com/?/api/v1/files/abcdefABCDEF"),
            None
        );
        assert_eq!(
            file_url("abc", "我的 文件.pdf"),
            "/api/v1/files/abc/%E6%88%91%E7%9A%84%20%E6%96%87%E4%BB%B6.pdf"
        );
    }

    #[test]
    fn mp4_duration() {
        // ftyp + moov(mvhd v0: timescale 1000, duration 12345；trak/tkhd 640x360)
        fn bx(ty: &[u8], body: &[u8]) -> Vec<u8> {
            let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
            v.extend_from_slice(ty);
            v.extend_from_slice(body);
            v
        }
        let mut mvhd = vec![0u8; 100];
        mvhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
        mvhd[16..20].copy_from_slice(&12345u32.to_be_bytes());
        let mut tkhd = vec![0u8; 84];
        tkhd[76..80].copy_from_slice(&(640u32 << 16).to_be_bytes());
        tkhd[80..84].copy_from_slice(&(360u32 << 16).to_be_bytes());
        let moov = bx(
            b"moov",
            &[bx(b"mvhd", &mvhd), bx(b"trak", &bx(b"tkhd", &tkhd))].concat(),
        );
        let file = [bx(b"ftyp", b"isom\0\0\0\0isom"), moov].concat();
        assert_eq!(mp4_meta(&file), (Some(12345), Some(640), Some(360)));
    }
}
