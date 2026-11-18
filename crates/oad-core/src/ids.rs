//! 编号、令牌、时间。
use sha2::{Digest, Sha256};

const B62: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// 12 位 base62 随机串（页面、用户、工作区的编号）。
pub fn new_id() -> String {
    (0..12)
        .map(|_| B62[(rand::random::<u32>() % 62) as usize] as char)
        .collect()
}

/// 会话令牌：32 字节随机数的十六进制。
pub fn new_token() -> String {
    hex(&rand::random::<[u8; 32]>())
}

/// 库里只存令牌的哈希。
pub fn hash_token(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
