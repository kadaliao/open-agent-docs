//! 登录限流：同一个 IP + 邮箱在一段时间里失败太多次，先拒绝，不再验密码。
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct LoginLimiter {
    max: u32,
    window: Duration,
    fails: Mutex<HashMap<String, (u32, Instant)>>,
}

impl LoginLimiter {
    pub fn new(max: u32, window: Duration) -> Self {
        LoginLimiter {
            max,
            window,
            fails: Mutex::new(HashMap::new()),
        }
    }

    /// 被限了返回还要等多久。
    pub fn check(&self, key: &str) -> Option<Duration> {
        let m = self.fails.lock().unwrap();
        let (n, since) = m.get(key)?;
        let left = self.window.checked_sub(since.elapsed())?;
        (*n >= self.max).then_some(left)
    }

    pub fn fail(&self, key: &str) {
        let mut m = self.fails.lock().unwrap();
        if m.len() > 10_000 {
            let w = self.window;
            m.retain(|_, (_, since)| since.elapsed() < w);
        }
        let e = m.entry(key.to_string()).or_insert((0, Instant::now()));
        if e.1.elapsed() >= self.window {
            *e = (0, Instant::now());
        }
        e.0 += 1;
    }

    pub fn success(&self, key: &str) {
        self.fails.lock().unwrap().remove(key);
    }
}

/// 每个调用者每分钟最多多少次请求（固定窗口）。调用者按会话 / token（取哈希）认，没有就按 IP。
pub struct RateLimiter {
    per_minute: u32,
    hits: Mutex<HashMap<String, (u32, Instant)>>,
}

/// 一次请求后的额度：(上限, 还剩, 几秒后重置)；超了 ok = false。
pub struct Quota {
    pub ok: bool,
    pub limit: u32,
    pub remaining: u32,
    pub reset: u64,
}

impl RateLimiter {
    pub fn new(per_minute: u32) -> Self {
        RateLimiter {
            per_minute,
            hits: Mutex::new(HashMap::new()),
        }
    }

    pub fn enabled(&self) -> bool {
        self.per_minute > 0
    }

    pub fn hit(&self, key: &str) -> Quota {
        let window = Duration::from_secs(60);
        let mut m = self.hits.lock().unwrap();
        if m.len() > 100_000 {
            m.retain(|_, (_, since)| since.elapsed() < window);
        }
        let e = m.entry(key.to_string()).or_insert((0, Instant::now()));
        if e.1.elapsed() >= window {
            *e = (0, Instant::now());
        }
        e.0 = e.0.saturating_add(1);
        let reset = window.saturating_sub(e.1.elapsed()).as_secs().max(1);
        Quota {
            ok: e.0 <= self.per_minute,
            limit: self.per_minute,
            remaining: self.per_minute.saturating_sub(e.0),
            reset,
        }
    }
}
