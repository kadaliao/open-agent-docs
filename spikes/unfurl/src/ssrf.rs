//! SSRF 防护：判断一个地址能不能让服务器去连。
//!
//! 服务器替用户抓网址，用户可以填 `http://127.0.0.1:6379`、`http://169.254.169.254/`（云主机元数据）
//! 之类的地址来探内网。这里只放行公网单播地址。
//!
//! 写法要点：
//! - 十六进制 / 十进制 / 八进制的 IPv4 写法（`0x7f000001`、`2130706433`、`0177.0.0.1`）由 `url` 库按 WHATWG
//!   规则规范化成普通的 IPv4，所以这里只需要处理 `IpAddr`。
//! - IPv6 里嵌着 IPv4 的形式（`::ffff:a.b.c.d`、`::a.b.c.d`、NAT64 `64:ff9b::/96`、6to4 `2002::/16`）
//!   取出里面的 IPv4 再判断一次。
//! - 真正连接用的地址在 `fetch.rs` 的自定义 DNS 解析器里再检查一次，防 DNS rebinding。

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use url::{Host, Url};

/// 返回 `None` 表示可以连；`Some(原因)` 表示拒绝。
pub fn classify_ip(ip: IpAddr) -> Option<&'static str> {
    match ip {
        IpAddr::V4(v4) => classify_v4(v4),
        IpAddr::V6(v6) => classify_v6(v6),
    }
}

pub fn is_public_ip(ip: IpAddr) -> bool {
    classify_ip(ip).is_none()
}

fn in_v4(ip: Ipv4Addr, net: [u8; 4], prefix: u32) -> bool {
    let mask = if prefix == 0 { 0 } else { u32::MAX << (32 - prefix) };
    (u32::from(ip) & mask) == (u32::from(Ipv4Addr::from(net)) & mask)
}

fn classify_v4(ip: Ipv4Addr) -> Option<&'static str> {
    const BLOCKS: &[([u8; 4], u32, &str)] = &[
        ([0, 0, 0, 0], 8, "this-network / unspecified"),
        ([10, 0, 0, 0], 8, "private (10/8)"),
        ([100, 64, 0, 0], 10, "carrier-grade NAT (100.64/10)"),
        ([127, 0, 0, 0], 8, "loopback"),
        ([169, 254, 0, 0], 16, "link-local (incl. cloud metadata)"),
        ([172, 16, 0, 0], 12, "private (172.16/12)"),
        ([192, 0, 0, 0], 24, "IETF protocol assignments"),
        ([192, 0, 2, 0], 24, "documentation (TEST-NET-1)"),
        ([192, 88, 99, 0], 24, "6to4 relay anycast"),
        ([192, 168, 0, 0], 16, "private (192.168/16)"),
        ([198, 18, 0, 0], 15, "benchmarking (198.18/15)"),
        ([198, 51, 100, 0], 24, "documentation (TEST-NET-2)"),
        ([203, 0, 113, 0], 24, "documentation (TEST-NET-3)"),
        ([224, 0, 0, 0], 4, "multicast"),
        ([240, 0, 0, 0], 4, "reserved / broadcast"),
    ];
    BLOCKS.iter().find(|(net, p, _)| in_v4(ip, *net, *p)).map(|(_, _, why)| *why)
}

fn classify_v6(ip: Ipv6Addr) -> Option<&'static str> {
    let seg = ip.segments();
    if ip.is_unspecified() {
        return Some("unspecified (::)");
    }
    if ip.is_loopback() {
        return Some("loopback (::1)");
    }
    // ::ffff:a.b.c.d（IPv4-mapped）
    if let Some(v4) = ip.to_ipv4_mapped() {
        return classify_v4(v4).map(|_| "IPv4-mapped address of a blocked IPv4");
    }
    // ::a.b.c.d（已废弃的 IPv4-compatible），前 96 位全 0
    if seg[..6].iter().all(|s| *s == 0) {
        let v4 = Ipv4Addr::new((seg[6] >> 8) as u8, seg[6] as u8, (seg[7] >> 8) as u8, seg[7] as u8);
        return classify_v4(v4).map(|_| "IPv4-compatible address of a blocked IPv4").or(Some("IPv4-compatible (deprecated)"));
    }
    // NAT64 64:ff9b::/96：里面的 IPv4 是内网就拒绝
    if seg[0] == 0x64 && seg[1] == 0xff9b && seg[2..6].iter().all(|s| *s == 0) {
        let v4 = Ipv4Addr::new((seg[6] >> 8) as u8, seg[6] as u8, (seg[7] >> 8) as u8, seg[7] as u8);
        return classify_v4(v4).map(|_| "NAT64 address of a blocked IPv4");
    }
    // 64:ff9b:1::/48 本地 NAT64
    if seg[0] == 0x64 && seg[1] == 0xff9b && seg[2] == 1 {
        return Some("local-use NAT64 (64:ff9b:1::/48)");
    }
    // 6to4 2002::/16：第 2、3 段是 IPv4
    if seg[0] == 0x2002 {
        let v4 = Ipv4Addr::new((seg[1] >> 8) as u8, seg[1] as u8, (seg[2] >> 8) as u8, seg[2] as u8);
        return classify_v4(v4).map(|_| "6to4 address of a blocked IPv4");
    }
    if seg[0] & 0xff00 == 0xff00 {
        return Some("multicast (ff00::/8)");
    }
    if seg[0] & 0xfe00 == 0xfc00 {
        return Some("unique local (fc00::/7)");
    }
    if seg[0] & 0xffc0 == 0xfe80 {
        return Some("link-local (fe80::/10)");
    }
    if seg[0] & 0xffc0 == 0xfec0 {
        return Some("site-local (fec0::/10, deprecated)");
    }
    if seg[0] == 0x0100 && seg[1..4].iter().all(|s| *s == 0) {
        return Some("discard-only (100::/64)");
    }
    if seg[0] == 0x2001 && seg[1] == 0x0db8 {
        return Some("documentation (2001:db8::/32)");
    }
    // 2001::/23 是 IETF 协议地址（Teredo 2001::/32 也在里面，Teredo 里藏着混淆过的 IPv4，直接拒绝）
    if seg[0] == 0x2001 && seg[1] < 0x0200 {
        return Some("IETF protocol assignment (2001::/23, incl. Teredo)");
    }
    if seg[0] == 0x3fff && seg[1] < 0x1000 {
        return Some("documentation (3fff::/20)");
    }
    // 剩下的只认全球单播 2000::/3
    if seg[0] & 0xe000 != 0x2000 {
        return Some("not global unicast");
    }
    None
}

/// 检查网址本身（还没解析 DNS）：协议、主机名、IP 字面量。
/// 域名要等解析出地址才能下结论，这里只拦明显不行的。
pub fn check_url(url: &Url) -> Result<(), String> {
    match url.scheme() {
        "http" | "https" => {}
        s => return Err(format!("scheme {s} not allowed")),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("credentials in url not allowed".into());
    }
    match url.host() {
        None => Err("missing host".into()),
        Some(Host::Ipv4(v4)) => classify_v4(v4).map_or(Ok(()), |why| Err(format!("{v4}: {why}"))),
        Some(Host::Ipv6(v6)) => classify_v6(v6).map_or(Ok(()), |why| Err(format!("{v6}: {why}"))),
        Some(Host::Domain(d)) => check_domain(d),
    }
}

/// 一些名字不用解析就知道不能连。
pub fn check_domain(domain: &str) -> Result<(), String> {
    let d = domain.trim_end_matches('.').to_ascii_lowercase();
    if d.is_empty() {
        return Err("empty host".into());
    }
    if d == "localhost" || d.ends_with(".localhost") {
        return Err(format!("{d}: localhost"));
    }
    // 单段主机名（`intranet`、`redis`）只会是内网名字；走代理时代理那边可能解析得出来。
    if !d.contains('.') {
        return Err(format!("{d}: single-label host name"));
    }
    for suffix in [".local", ".internal", ".lan", ".home.arpa", ".intranet", ".corp"] {
        if d.ends_with(suffix) {
            return Err(format!("{d}: internal-only TLD"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url_ok(s: &str) -> bool {
        check_url(&Url::parse(s).unwrap()).is_ok()
    }

    #[test]
    fn literal_urls() {
        // 回环的各种写法：url 库会把十六进制 / 十进制 / 八进制 / 简写都规范成点分十进制
        for s in [
            "http://127.0.0.1",
            "http://127.0.0.1:6379/",
            "http://[::1]/",
            "http://[::ffff:127.0.0.1]/",
            "http://[::ffff:7f00:1]/",
            "http://[0:0:0:0:0:ffff:127.0.0.1]/",
            "http://[::127.0.0.1]/",
            "http://0x7f000001/",
            "http://0x7f.0x0.0x0.0x1/",
            "http://2130706433/",
            "http://0177.0.0.1/",
            "http://127.1/",
            "http://0/",
            "http://0.0.0.0/",
            "http://localhost/",
            "http://LOCALHOST./",
            "http://foo.localhost/",
            "http://169.254.169.254/latest/meta-data/",
            "http://[fe80::1]/",
            "http://[fd00::1]/",
            "http://[ff02::1]/",
            "http://10.0.0.1/",
            "http://167772161/", // 10.0.0.1 十进制
            "http://172.16.5.4/",
            "http://192.168.1.1/",
            "http://100.64.0.1/",
            "http://255.255.255.255/",
            "http://224.0.0.1/",
            "http://[64:ff9b::7f00:1]/",
            "http://[2002:7f00:1::]/",
            "http://[2001:db8::1]/",
            "http://redis:6379/",
            "http://printer.local/",
            "ftp://example.com/",
            "file:///etc/passwd",
            "gopher://example.com/",
            "http://user:pass@example.com/",
        ] {
            assert!(!url_ok(s), "{s} should be blocked");
        }
        for s in [
            "http://example.com/",
            "https://x.com/jack/status/20",
            "http://8.8.8.8/",
            "http://134744072/", // 8.8.8.8 十进制
            "http://[2606:4700:4700::1111]/",
            "http://[::ffff:8.8.8.8]/",
            "http://[64:ff9b::808:808]/",
        ] {
            assert!(url_ok(s), "{s} should be allowed");
        }
    }

    #[test]
    fn ip_ranges() {
        let blocked = [
            "0.1.2.3",
            "10.255.255.255",
            "100.127.255.255",
            "127.255.0.1",
            "169.254.0.1",
            "172.31.255.255",
            "192.0.0.8",
            "192.0.2.1",
            "192.168.0.1",
            "198.18.0.1",
            "198.19.255.255",
            "198.51.100.7",
            "203.0.113.9",
            "239.255.255.250",
            "240.0.0.1",
            "::",
            "::1",
            "fc00::1",
            "fdff::1",
            "fe80::abcd",
            "fec0::1",
            "ff05::2",
            "100::1",
            "2001::1",
            "2001:0:4136:e378::1",
            "::ffff:10.0.0.1",
            "::ffff:192.168.1.1",
            "::ffff:169.254.169.254",
            "64:ff9b:1::1",
        ];
        for s in blocked {
            assert!(!is_public_ip(s.parse().unwrap()), "{s} should be blocked");
        }
        let allowed = [
            "1.1.1.1",
            "8.8.8.8",
            "100.63.255.255",
            "100.128.0.1",
            "172.15.0.1",
            "172.32.0.1",
            "192.169.0.1",
            "198.17.255.255",
            "198.20.0.1",
            "223.255.255.255",
            "28.0.0.4",
            "2001:4860:4860::8888",
            "2606:4700::6810:84e5",
            "::ffff:1.1.1.1",
        ];
        for s in allowed {
            assert!(is_public_ip(s.parse().unwrap()), "{s} should be allowed");
        }
    }
}
