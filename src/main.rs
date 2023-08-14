mod average_updater;
mod readable_string;

use average_updater::AverageUpdater;
use chrono::Duration;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest;
use rss::Channel;
use serde_derive::{Deserialize, Serialize};
use serde_yaml;
use std::fs::File;

use crate::readable_string::ReadableString;

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
    let mut updater = AverageUpdater::new();
    let client = megalodon::generator(
        megalodon::SNS::Mastodon,
        base_url.clone(),
        Some(feed.token.clone()),
        None,
    );
    loop {
        let channel = fetch_feed(&feed.url).await?;
        // 1番目の記事が存在しない場合は最大まで待機
        let Some(item) = channel.items().get(0) else{
            sleep(updater.get_next_wait()).await;
            continue;
        };
        // 2番目の記事が存在する場合 かつ 最後の更新時間が存在しない場合は更新時間をセット
        if let Some(before) = channel.items().get(1) {
            if updater.last_time().is_none() {
                updater.update_from_rfc2822(before.pub_date().unwrap());
            }
        };
        // タイトルを取得して以前と同じなら待機
        let title = item.title().unwrap_or_default().to_string();
        if updater.last_title().as_ref() == Some(&title) {
            sleep(updater.get_next_wait()).await;
            continue;
        }
        updater.set_last_title(Some(title));
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
        updater.update_from_rfc2822(item.pub_date().unwrap());
        sleep(updater.get_next_wait()).await;
    }
}

async fn sleep(duration: Duration) {
    println!("sleep {}", duration.to_readable_string());
    tokio::time::sleep(duration.to_std().unwrap()).await;
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
