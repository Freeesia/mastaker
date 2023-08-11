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
    rss_url: String,
    mastodon_url: String,
    mastodon_token: String,
}

async fn fetch_rss(url: &str) -> Result<Channel, reqwest::Error> {
    let content = reqwest::get(url).await?.text().await?;
    let channel = content.parse::<Channel>().unwrap();
    Ok(channel)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("local.yml")?;
    let config: Config = serde_yaml::from_reader(file)?;
    let re = Regex::new(r"[^\w]").unwrap(); // 単語文字以外の文字にマッチする正規表現
    let client = megalodon::generator(
        megalodon::SNS::Mastodon,
        config.mastodon_url,
        Some(config.mastodon_token),
        None,
    );

    loop {
        let channel = fetch_rss(&config.rss_url).await?;
        for item in channel.items().iter().skip(1).take(1) {
            let mut content_parts = vec![
                item.title().unwrap_or("").to_string(),
                item.link().unwrap_or("").to_string(),
                item.categories()
                    .iter()
                    .map(|c| format!("#{}", re.replace_all(&c.name(), "_")))
                    .collect::<Vec<String>>()
                    .join(" ")
            ];
            // 空の文字列を削除
            content_parts.retain(|part| !part.is_empty());    
            client.post_status(content_parts.join("\n"), None).await?;
        }
        sleep(Duration::from_secs(3600)).await; // 1時間毎にチェック
    }
}
