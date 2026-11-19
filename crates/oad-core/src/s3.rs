//! S3 兼容的文件存储（AWS S3、MinIO、R2、OSS 的 S3 接口……）：`OAD_BLOB_URL=s3://<bucket>/<前缀>?region=..&endpoint=..`，
//! 密钥从 `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`（可选 `AWS_SESSION_TOKEN`）读。给了 endpoint 用路径式地址
//! （`<endpoint>/<bucket>/<key>`，MinIO 这样），没给用 AWS 的虚拟主机式（`https://<bucket>.s3.<region>.amazonaws.com/<key>`）。
//! 签名是 SigV4。
use crate::blob::BlobStore;
use crate::{Error, Result};
use async_trait::async_trait;
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::time::Duration;

/// S3 的配置。
#[derive(Clone, Debug)]
pub struct S3Config {
    pub bucket: String,
    /// 键的前缀（不带开头的 /；可以为空）
    pub prefix: String,
    pub region: String,
    /// 自己的服务地址（MinIO 之类），给了就用路径式地址
    pub endpoint: Option<String>,
    pub access_key: String,
    pub secret_key: String,
    pub session_token: Option<String>,
}

impl S3Config {
    /// 从 `s3://bucket/prefix?region=..&endpoint=..` 和环境变量读。
    pub fn from_url(u: &str) -> Result<S3Config> {
        let url =
            url::Url::parse(u).map_err(|e| Error::BadRequest(format!("OAD_BLOB_URL 不对：{e}")))?;
        if url.scheme() != "s3" {
            return Err(Error::BadRequest(
                "OAD_BLOB_URL 要是 s3://bucket/前缀".into(),
            ));
        }
        let bucket = url.host_str().unwrap_or("").to_string();
        if bucket.is_empty() {
            return Err(Error::BadRequest("OAD_BLOB_URL 里没有 bucket".into()));
        }
        let q = |k: &str| {
            url.query_pairs()
                .find(|(x, _)| x == k)
                .map(|(_, v)| v.to_string())
        };
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Ok(S3Config {
            bucket,
            prefix: url.path().trim_matches('/').to_string(),
            region: q("region")
                .or_else(|| env("AWS_REGION"))
                .unwrap_or_else(|| "us-east-1".into()),
            endpoint: q("endpoint").map(|e| e.trim_end_matches('/').to_string()),
            access_key: env("AWS_ACCESS_KEY_ID")
                .ok_or_else(|| Error::BadRequest("S3 要设 AWS_ACCESS_KEY_ID".into()))?,
            secret_key: env("AWS_SECRET_ACCESS_KEY")
                .ok_or_else(|| Error::BadRequest("S3 要设 AWS_SECRET_ACCESS_KEY".into()))?,
            session_token: env("AWS_SESSION_TOKEN"),
        })
    }
}

pub struct S3Blobs {
    cfg: S3Config,
    http: reqwest::Client,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn sha_hex(b: &[u8]) -> String {
    hex(&Sha256::digest(b))
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut m = <Hmac<Sha256> as KeyInit>::new_from_slice(key).expect("HMAC 接受任意长度的密钥");
    m.update(data);
    m.finalize().into_bytes().to_vec()
}

/// 路径按 SigV4 的规矩编码（`/` 不编码）。
fn uri_encode(s: &str, keep_slash: bool) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) || keep_slash && b == b'/' {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}

/// 要签名的请求。headers 要包含 host、x-amz-date、x-amz-content-sha256（小写名字）。
pub struct SignInput<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub query: &'a [(String, String)],
    pub headers: &'a [(String, String)],
    pub payload_hash: &'a str,
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub region: &'a str,
    /// 20130524T000000Z
    pub amz_date: &'a str,
}

/// SigV4 签名：返回 Authorization 头的值。
pub fn sign_v4(i: &SignInput) -> String {
    let SignInput {
        method,
        path,
        query,
        headers,
        payload_hash,
        access_key,
        secret_key,
        region,
        amz_date,
    } = *i;
    let date = &amz_date[..8];
    let mut q: Vec<(String, String)> = query
        .iter()
        .map(|(k, v)| (uri_encode(k, false), uri_encode(v, false)))
        .collect();
    q.sort();
    let canon_query = q
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    let mut hs: Vec<(String, String)> = headers
        .iter()
        .map(|(k, v)| (k.to_lowercase(), v.trim().to_string()))
        .collect();
    hs.sort();
    let canon_headers: String = hs.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    let signed = hs
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let canonical = format!(
        "{method}\n{}\n{canon_query}\n{canon_headers}\n{signed}\n{payload_hash}",
        uri_encode(path, true)
    );
    let scope = format!("{date}/{region}/s3/aws4_request");
    let to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        sha_hex(canonical.as_bytes())
    );
    let k = hmac(format!("AWS4{secret_key}").as_bytes(), date.as_bytes());
    let k = hmac(&k, region.as_bytes());
    let k = hmac(&k, b"s3");
    let k = hmac(&k, b"aws4_request");
    let sig = hex(&hmac(&k, to_sign.as_bytes()));
    format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders={signed}, Signature={sig}"
    )
}

fn amz_now() -> String {
    let t = jiff::Timestamp::now();
    t.strftime("%Y%m%dT%H%M%SZ").to_string()
}

impl S3Blobs {
    pub fn new(cfg: S3Config) -> Result<S3Blobs> {
        crate::notify::install_tls();
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| Error::Internal(e.to_string()))?;
        Ok(S3Blobs { cfg, http })
    }

    /// (完整网址, Host 头, 签名用的路径)
    fn locate(&self, key: &str) -> (String, String, String) {
        let obj = if self.cfg.prefix.is_empty() {
            key.to_string()
        } else {
            format!("{}/{key}", self.cfg.prefix)
        };
        match &self.cfg.endpoint {
            Some(ep) => {
                let host = ep
                    .split("://")
                    .nth(1)
                    .unwrap_or(ep)
                    .split('/')
                    .next()
                    .unwrap_or("")
                    .to_string();
                let path = format!("/{}/{obj}", self.cfg.bucket);
                (format!("{ep}{}", uri_encode(&path, true)), host, path)
            }
            None => {
                let host = format!("{}.s3.{}.amazonaws.com", self.cfg.bucket, self.cfg.region);
                let path = format!("/{obj}");
                (
                    format!("https://{host}{}", uri_encode(&path, true)),
                    host,
                    path,
                )
            }
        }
    }

    async fn send(
        &self,
        method: reqwest::Method,
        key: &str,
        body: Option<Vec<u8>>,
        range: Option<String>,
    ) -> Result<reqwest::Response> {
        crate::blob::check_key(key)?;
        let (url, host, path) = self.locate(key);
        let payload = sha_hex(body.as_deref().unwrap_or_default());
        let date = amz_now();
        let mut headers = vec![
            ("host".to_string(), host),
            ("x-amz-content-sha256".to_string(), payload.clone()),
            ("x-amz-date".to_string(), date.clone()),
        ];
        if let Some(r) = &range {
            headers.push(("range".into(), r.clone()));
        }
        if let Some(t) = &self.cfg.session_token {
            headers.push(("x-amz-security-token".into(), t.clone()));
        }
        let auth = sign_v4(&SignInput {
            method: method.as_str(),
            path: &path,
            query: &[],
            headers: &headers,
            payload_hash: &payload,
            access_key: &self.cfg.access_key,
            secret_key: &self.cfg.secret_key,
            region: &self.cfg.region,
            amz_date: &date,
        });
        let mut req = self
            .http
            .request(method, &url)
            .header("authorization", auth);
        for (k, v) in headers.into_iter().filter(|(k, _)| k != "host") {
            req = req.header(k, v);
        }
        if let Some(b) = body {
            req = req.body(b);
        }
        req.send()
            .await
            .map_err(|e| Error::Storage(format!("S3 请求失败：{e}")))
    }

    fn check(r: reqwest::Response) -> Result<Option<reqwest::Response>> {
        match r.status().as_u16() {
            200..=299 => Ok(Some(r)),
            404 => Ok(None),
            // Range 起点超过长度：当空
            416 => Ok(None),
            s => Err(Error::Storage(format!("S3 返回 {s}"))),
        }
    }
}

#[async_trait]
impl BlobStore for S3Blobs {
    async fn put(&self, key: &str, data: Vec<u8>) -> Result<()> {
        // 内容寻址：已经有了就不再传
        if self.len(key).await?.is_some() {
            return Ok(());
        }
        let r = self
            .send(reqwest::Method::PUT, key, Some(data), None)
            .await?;
        Self::check(r)?.ok_or_else(|| Error::Storage("S3 bucket 不存在".into()))?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let r = self.send(reqwest::Method::GET, key, None, None).await?;
        match Self::check(r)? {
            Some(r) => Ok(Some(
                r.bytes()
                    .await
                    .map_err(|e| Error::Storage(e.to_string()))?
                    .to_vec(),
            )),
            None => Ok(None),
        }
    }

    async fn range(&self, key: &str, start: u64, end: u64) -> Result<Option<Vec<u8>>> {
        if end <= start {
            return Ok(self.len(key).await?.map(|_| Vec::new()));
        }
        let r = self
            .send(
                reqwest::Method::GET,
                key,
                None,
                Some(format!("bytes={start}-{}", end - 1)),
            )
            .await?;
        if r.status().as_u16() == 416 {
            return Ok(self.len(key).await?.map(|_| Vec::new()));
        }
        match Self::check(r)? {
            Some(r) => Ok(Some(
                r.bytes()
                    .await
                    .map_err(|e| Error::Storage(e.to_string()))?
                    .to_vec(),
            )),
            None => Ok(None),
        }
    }

    async fn len(&self, key: &str) -> Result<Option<u64>> {
        let r = self.send(reqwest::Method::HEAD, key, None, None).await?;
        Ok(Self::check(r)?.and_then(|r| {
            r.headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
        }))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let r = self.send(reqwest::Method::DELETE, key, None, None).await?;
        Self::check(r)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AWS 文档里的例子（GET Object，带 Range）。
    #[test]
    fn sigv4_matches_aws_example() {
        let headers = vec![
            (
                "host".to_string(),
                "examplebucket.s3.amazonaws.com".to_string(),
            ),
            ("range".to_string(), "bytes=0-9".to_string()),
            (
                "x-amz-content-sha256".to_string(),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            ),
            ("x-amz-date".to_string(), "20130524T000000Z".to_string()),
        ];
        let auth = sign_v4(&SignInput {
            method: "GET",
            path: "/test.txt",
            query: &[],
            headers: &headers,
            payload_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            access_key: "AKIAIOSFODNN7EXAMPLE",
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            region: "us-east-1",
            amz_date: "20130524T000000Z",
        });
        assert_eq!(
            auth,
            "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, \
             SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, \
             Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }

    #[test]
    fn config_from_url() {
        // SAFETY: 测试里设环境变量（这个测试不和别的读同一个变量的测试并发）
        unsafe {
            std::env::set_var("AWS_ACCESS_KEY_ID", "ak");
            std::env::set_var("AWS_SECRET_ACCESS_KEY", "sk");
        }
        let c = S3Config::from_url(
            "s3://my-bucket/oad/blobs?region=eu-west-1&endpoint=http://127.0.0.1:9000/",
        )
        .unwrap();
        assert_eq!(
            (c.bucket.as_str(), c.prefix.as_str(), c.region.as_str()),
            ("my-bucket", "oad/blobs", "eu-west-1")
        );
        assert_eq!(c.endpoint.as_deref(), Some("http://127.0.0.1:9000"));
        let b = S3Blobs::new(c).unwrap();
        assert_eq!(
            b.locate("abc").0,
            "http://127.0.0.1:9000/my-bucket/oad/blobs/abc"
        );
        assert!(S3Config::from_url("https://x").is_err());
    }
}
