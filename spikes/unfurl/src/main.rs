use std::time::Instant;

use clap::Parser;
use unfurl::{FetchConfig, Unfurler};

/// 把链接解析成卡片数据，打印 JSON。
#[derive(Parser)]
#[command(name = "unfurl", version)]
struct Args {
    /// 一个或多个网址
    #[arg(required = true)]
    urls: Vec<String>,
    /// 走代理（覆盖 OAD_FETCH_PROXY），例如 http://127.0.0.1:7890
    #[arg(long)]
    proxy: Option<String>,
    /// 不打印 JSON，只打印一行摘要
    #[arg(long, short)]
    summary: bool,
    /// 不输出 attempts
    #[arg(long)]
    no_attempts: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let mut cfg = FetchConfig::from_env();
    if args.proxy.is_some() {
        cfg.proxy = args.proxy.clone();
    }
    let proxied = cfg.proxy.is_some();
    let u = match Unfurler::new(cfg) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };
    let mut failed = false;
    for url in &args.urls {
        let t = Instant::now();
        let r = u.unfurl(url).await;
        let ms = t.elapsed().as_millis();
        match r {
            Ok(mut card) => {
                if args.summary {
                    let steps: Vec<String> =
                        card.attempts.iter().map(|a| format!("{}{}({}ms)", a.step, if a.ok { "✓" } else { "✗" }, a.ms)).collect();
                    println!(
                        "OK  {ms:>5}ms {:?}/{:?}/{:?} {} | {} | {}",
                        card.provider,
                        card.kind,
                        card.source,
                        url,
                        card.title.as_deref().unwrap_or("-"),
                        steps.join(" ")
                    );
                } else {
                    if args.no_attempts {
                        card.attempts.clear();
                    }
                    println!("{}", serde_json::to_string_pretty(&card).unwrap());
                    eprintln!("elapsed: {ms} ms{}", if proxied { " (via proxy)" } else { "" });
                }
            }
            Err(e) => {
                failed = true;
                if args.summary {
                    println!("ERR {ms:>5}ms {url} | {e}");
                } else {
                    eprintln!("error after {ms} ms: {e}");
                }
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}
