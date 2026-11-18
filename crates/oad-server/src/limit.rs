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
