//! 微信公众号文章（mp.weixin.qq.com）：网页本身有 og:title / og:image，
//! 公众号名称、头像、发布时间在页面脚本的变量里（`var nickname = …`、`var ct = "…"`）。

use regex::Regex;
use url::Url;

use super::{Log, unix_to_rfc3339};
use crate::error::{Error, Result};
use crate::fetch::{Fetcher, RequestOpts};
use crate::html;
use crate::model::{Author, Embed, Image, Kind, Provider, Unfurl, WechatEmbed};
use crate::providers::generic::{self, Fetchedpage};

pub fn matches(url: &Url) -> bool {
    url.host_str() == Some("mp.weixin.qq.com") && url.path().starts_with("/s")
}

/// 取 `var name = "..."`、`var name = htmlDecode("...")`、`name: '...'` 这几种写法的值（第一个非空的）。
pub fn script_var(html_src: &str, name: &str) -> Option<String> {
    script_vars(html_src, name).into_iter().next()
}

/// 同上，返回所有匹配（页面里同一个名字常出现好几次，格式还不一样）。
pub fn script_vars(html_src: &str, name: &str) -> Vec<String> {
    let re = Regex::new(&format!(r#"(?:var\s+{0}\s*=|\b{0}\s*[:=])\s*(?:htmlDecode\()?\s*(?:"([^"]*)"|'([^']*)')"#, regex::escape(name)))
        .unwrap();
    re.captures_iter(html_src)
        .filter_map(|c| {
            let v = c.get(1).or_else(|| c.get(2))?.as_str();
            let v = v
                .replace("\\x26amp;", "&")
                .replace("\\x26", "&")
                .replace("&amp;", "&")
                .replace("\\x0a", " ")
                .replace("\\/", "/")
                .replace("&quot;", "\"")
                .replace("&#39;", "'");
            let v = html::clean(&v);
            (!v.is_empty()).then_some(v)
        })
        .collect()
}

fn first_number(html_src: &str, names: &[&str]) -> Option<i64> {
    names.iter().find_map(|n| script_vars(html_src, n).into_iter().find_map(|v| v.parse::<i64>().ok().filter(|t| *t > 0)))
}

pub async fn unfurl(f: &Fetcher, input: &Url, log: &mut Log) -> Result<Unfurl> {
    let page = log.step("html", generic::fetch_page(f, input, RequestOpts::html())).await.ok_or_else(|| log.all_failed())?;
    let page = match page {
        Fetchedpage::Html(p) => p,
        Fetchedpage::NotHtml(u) => return Ok(*u),
    };
    let src = &page.html;
    if src.contains("环境异常") || src.contains("访问过于频繁") || src.contains("完成验证后即可继续访问") {
        let e: Result<()> = Err(Error::Parse("wechat anti-bot / captcha page".into()));
        log.record("wechat.parse", 0, &e);
        return Err(log.all_failed());
    }
    if src.contains("该内容已被发布者删除") || src.contains("此内容因违规无法查看") {
        return Err(Error::NotFound);
    }
    let mut u = page.meta.to_unfurl(input.as_str(), &page.fetched.url);
    u.provider = Provider::Wechat;
    u.kind = Kind::Article;
    u.lang.get_or_insert_with(|| "zh-CN".into());
    if u.title.is_none() {
        u.title = script_var(src, "msg_title");
    }
    if u.description.is_none() {
        u.description = script_var(src, "msg_desc").map(|d| html::truncate(&d, 500));
    }
    if u.thumbnail.is_none() {
        u.thumbnail = script_var(src, "msg_cdn_url").map(Image::new);
    }
    let account = script_var(src, "nickname").or_else(|| script_var(src, "nick_name"));
    let avatar = script_var(src, "round_head_img").or_else(|| script_var(src, "hd_head_img"));
    if u.published_at.is_none() {
        u.published_at = first_number(src, &["ct", "create_timestamp", "create_time"]).map(unix_to_rfc3339);
    }
    let author_name = u.author.as_ref().and_then(|a| a.name.clone()).or_else(|| account.clone());
    u.author = Some(Author { name: author_name, avatar: avatar.clone(), ..Default::default() });
    u.site_name = account.clone().or_else(|| Some("微信公众平台".into()));
    u.embed = Some(Embed::Wechat(WechatEmbed { account, account_avatar: avatar }));
    if u.title.is_none() {
        return Err(Error::Parse("no title on wechat page".into()));
    }
    Ok(u)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vars() {
        let s = r#"var nickname = htmlDecode("某某公众号");
            var ct = "1700000000";
            var msg_cdn_url = "http://mmbiz.qpic.cn/a/0?wx_fmt=jpeg";
            var msg_title = '标题 \x26amp; 副标题'.html(false);
            window.x = { round_head_img: "http://wx.qlogo.cn/mmhead/abc/0" };"#;
        assert_eq!(script_var(s, "nickname").as_deref(), Some("某某公众号"));
        assert_eq!(script_var(s, "ct").as_deref(), Some("1700000000"));
        assert_eq!(script_var(s, "msg_title").as_deref(), Some("标题 & 副标题"));
        assert_eq!(script_var(s, "round_head_img").as_deref(), Some("http://wx.qlogo.cn/mmhead/abc/0"));
        assert!(script_var(s, "missing").is_none());
        let s2 = "create_time: '2021-04-12 08:05', x; create_time: '1618185900' * 1";
        assert_eq!(first_number(s2, &["ct", "create_time"]), Some(1618185900));
        assert!(matches(&Url::parse("https://mp.weixin.qq.com/s/AbCdEf").unwrap()));
        assert!(matches(&Url::parse("https://mp.weixin.qq.com/s?__biz=MzA&mid=1").unwrap()));
    }
}
