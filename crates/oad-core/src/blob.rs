//! 存文件内容的地方。按键存取，键是内容的 SHA-256（十六进制）或者由它派生的（缩略图 `<hash>.w640.webp`）。
//! 首版是本地目录（[`FsBlobs`]），S3 之类以后实现同一个 trait。
use crate::{Error, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

#[async_trait]
pub trait BlobStore: Send + Sync + 'static {
    /// 存（同一个键已经有了就不动）。
    async fn put(&self, key: &str, data: Vec<u8>) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;
    /// [start, end) 这一段（end 超过长度时截到末尾）。
    async fn range(&self, key: &str, start: u64, end: u64) -> Result<Option<Vec<u8>>>;
    async fn len(&self, key: &str) -> Result<Option<u64>>;
    /// 删掉（没有也不报错）。
    async fn delete(&self, key: &str) -> Result<()>;
}

/// 键只能是字母数字和 `.` `-` `_`，不能拿来跳目录。
fn check_key(key: &str) -> Result<()> {
    if key.len() < 3
        || key.len() > 200
        || key.starts_with('.')
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err(Error::BadRequest(format!("不合法的内容键：{key}")));
    }
    Ok(())
}

/// 本地目录：`<root>/<键的前两位>/<键>`。写时先写临时文件再改名，读的人看不到写了一半的。
pub struct FsBlobs {
    root: PathBuf,
}

impl FsBlobs {
    pub fn new(root: impl Into<PathBuf>) -> Result<FsBlobs> {
        let root = root.into();
        std::fs::create_dir_all(&root)
            .map_err(|e| Error::Storage(format!("建 {}：{e}", root.display())))?;
        Ok(FsBlobs { root })
    }

    fn path(&self, key: &str) -> Result<PathBuf> {
        check_key(key)?;
        Ok(self.root.join(&key[..2]).join(key))
    }
}

fn io(e: std::io::Error) -> Error {
    Error::Storage(e.to_string())
}

#[async_trait]
impl BlobStore for FsBlobs {
    async fn put(&self, key: &str, data: Vec<u8>) -> Result<()> {
        let path = self.path(key)?;
        tokio::task::spawn_blocking(move || {
            if path.exists() {
                return Ok(());
            }
            let dir = path.parent().unwrap();
            std::fs::create_dir_all(dir).map_err(io)?;
            let tmp = dir.join(format!(
                ".{}.{}",
                path.file_name().unwrap().to_string_lossy(),
                rand::random::<u64>()
            ));
            std::fs::write(&tmp, &data).map_err(io)?;
            std::fs::rename(&tmp, &path).map_err(io)
        })
        .await
        .map_err(|e| Error::Internal(e.to_string()))?
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let path = self.path(key)?;
        match tokio::fs::read(&path).await {
            Ok(d) => Ok(Some(d)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(e)),
        }
    }

    async fn range(&self, key: &str, start: u64, end: u64) -> Result<Option<Vec<u8>>> {
        use std::io::{Read, Seek, SeekFrom};
        let path = self.path(key)?;
        tokio::task::spawn_blocking(move || {
            let mut f = match std::fs::File::open(&path) {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(io(e)),
            };
            let len = f.metadata().map_err(io)?.len();
            let end = end.min(len);
            if start >= end {
                return Ok(Some(Vec::new()));
            }
            f.seek(SeekFrom::Start(start)).map_err(io)?;
            let mut buf = vec![0; (end - start) as usize];
            f.read_exact(&mut buf).map_err(io)?;
            Ok(Some(buf))
        })
        .await
        .map_err(|e| Error::Internal(e.to_string()))?
    }

    async fn len(&self, key: &str) -> Result<Option<u64>> {
        let path = self.path(key)?;
        match tokio::fs::metadata(&path).await {
            Ok(m) => Ok(Some(m.len())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(e)),
        }
    }
    async fn delete(&self, key: &str) -> Result<()> {
        let p = self.path(key)?;
        match tokio::fs::remove_file(&p).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io(e)),
        }
    }
}

/// 放在内存里（测试、没配数据目录时）。
#[derive(Default)]
pub struct MemBlobs(Mutex<HashMap<String, std::sync::Arc<Vec<u8>>>>);

#[async_trait]
impl BlobStore for MemBlobs {
    async fn put(&self, key: &str, data: Vec<u8>) -> Result<()> {
        check_key(key)?;
        self.0
            .lock()
            .unwrap()
            .entry(key.to_string())
            .or_insert_with(|| std::sync::Arc::new(data));
        Ok(())
    }
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.0.lock().unwrap().get(key).map(|d| d.to_vec()))
    }
    async fn range(&self, key: &str, start: u64, end: u64) -> Result<Option<Vec<u8>>> {
        Ok(self.0.lock().unwrap().get(key).map(|d| {
            let end = (end as usize).min(d.len());
            let start = (start as usize).min(end);
            d[start..end].to_vec()
        }))
    }
    async fn len(&self, key: &str) -> Result<Option<u64>> {
        Ok(self.0.lock().unwrap().get(key).map(|d| d.len() as u64))
    }
    async fn delete(&self, key: &str) -> Result<()> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fs_blobs() {
        let dir = tempfile::tempdir().unwrap();
        let b = FsBlobs::new(dir.path()).unwrap();
        b.put("abcdef", b"hello world".to_vec()).await.unwrap();
        b.put("abcdef", b"ignored".to_vec()).await.unwrap();
        assert_eq!(b.get("abcdef").await.unwrap().unwrap(), b"hello world");
        assert_eq!(b.range("abcdef", 6, 100).await.unwrap().unwrap(), b"world");
        assert_eq!(b.len("abcdef").await.unwrap(), Some(11));
        assert_eq!(b.get("abcxyz").await.unwrap(), None);
        assert!(b.get("../etc/passwd").await.is_err());
        assert!(b.put("ab/../../x", vec![]).await.is_err());
    }
}
