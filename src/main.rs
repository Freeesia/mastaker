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

async fn post_to_mastodon(token: &str, url: &str, content: &str) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::new();
    client
        .post(url)
        .bearer_auth(token)
        .form(&[("status", content)])
        .send()
        .await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("config.yaml")?;
    let config: Config = serde_yaml::from_reader(file)?;

    loop {
        let channel = fetch_rss(&config.rss_url).await?;
        for item in channel.items().iter().take(1) {
            // 例として最新の1つだけを取得
            let content = format!(
                "{} - {}",
                item.title().unwrap_or(""),
                item.link().unwrap_or("")
            );
            post_to_mastodon(&config.mastodon_token, &config.mastodon_url, &content).await?;
        }
        sleep(Duration::from_secs(3600)).await; // 1時間毎にチェック
    }
}
