//! SSRF 防护：判断一个地址能不能让服务器去连。判断规则在 oad-unfurl（链接解析和 webhook 共用），
//! 这里加一个给 reqwest 用的 DNS 解析器：只返回公网地址，防 DNS rebinding。
pub use oad_unfurl::ssrf::*;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use std::net::SocketAddr;

/// 只返回公网地址的 DNS 解析器；一个都不剩就报错。
pub struct GuardedResolver;

#[derive(Debug, thiserror::Error)]
#[error("不允许连接的地址：{0}")]
struct Blocked(String);

impl Resolve for GuardedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            if let Err(why) = check_domain(&host) {
                return Err(Box::new(Blocked(why)) as Box<dyn std::error::Error + Send + Sync>);
            }
            let addrs: Vec<SocketAddr> =
                tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            let mut rejected = Vec::new();
            let ok: Vec<SocketAddr> = addrs
                .into_iter()
                .filter(|a| match classify_ip(a.ip()) {
                    None => true,
                    Some(why) => {
                        rejected.push(format!("{}: {why}", a.ip()));
                        false
                    }
                })
                .collect();
            if ok.is_empty() {
                let why = if rejected.is_empty() {
                    format!("{host}: 解析不出地址")
                } else {
                    format!("{host} -> {}", rejected.join(", "))
                };
                return Err(Box::new(Blocked(why)) as Box<dyn std::error::Error + Send + Sync>);
            }
            Ok(Box::new(ok.into_iter()) as Addrs)
        })
    }
}
