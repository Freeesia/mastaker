use once_cell::sync::Lazy;
use regex::Regex;
use reqwest;
use rss::Channel;
use serde_derive::{Deserialize, Serialize};
use serde_yaml;
use std::fs::File;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    base_url: String,
    feeds: Vec<Feed>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Feed {
    url: String,
    token: String,
}

static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^\w]").unwrap()); // 単語文字以外の文字にマッチする正規表現

async fn fetch_feed(url: &str) -> Result<Channel, reqwest::Error> {
    let content = reqwest::get(url).await?.text().await?;
    let channel = content.parse::<Channel>().unwrap();
    Ok(channel)
}

async fn process_feed(feed: &Feed, base_url: &String) -> Result<(), Box<dyn std::error::Error>> {
    let client = megalodon::generator(
        megalodon::SNS::Mastodon,
        base_url.clone(),
        Some(feed.token.clone()),
        None,
    );
    loop {
        let channel = fetch_feed(&feed.url).await?;
        for item in channel.items().iter().skip(1).take(1) {
            let mut content_parts = vec![
                item.title().unwrap_or("").to_string(),
                item.link().unwrap_or("").to_string(),
                item.categories()
                    .iter()
                    .map(|c| format!("#{}", TAG_RE.replace_all(&c.name(), "_")))
                    .collect::<Vec<String>>()
                    .join(" "),
            ];
            // 空の文字列を削除
            content_parts.retain(|part| !part.is_empty());
            let content = content_parts.join("\n");
            println!("{} -> \n{}", feed.url, content);
            client.post_status(content, None).await?;
        }
        sleep(Duration::from_secs(3600)).await; // 1時間毎にチェック
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("local.yml")?;
    let config: Config = serde_yaml::from_reader(file)?;

    let tasks: Vec<_> = config
        .feeds
        .iter()
        .map(|rss| process_feed(rss, &config.base_url))
        .collect();

    futures::future::join_all(tasks).await;
    Ok(())
}
