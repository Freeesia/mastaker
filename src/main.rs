mod average_updater;
mod posted_item;
mod ext_trait;

use average_updater::AverageUpdater;
use chrono::Duration;
use megalodon::megalodon::{PostStatusOutput, PostStatusInputOptions};
use posted_item::Entity as PostedItem;
use reqwest;
use rss::Channel;
use sea_orm::*;
use sea_orm_migration::SchemaManager;
use serde_derive::{Deserialize, Serialize};
use serde_yaml;
use std::{env, fs::File};

use crate::ext_trait::*;

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

const DATABASE_URL_ENV: &str = "DATABASE_URL";
const FEED_CONFIG_PATH_ENV: &str = "FEED_CONFIG_PATH";
const IS_DRY_RUN_ENV: &str = "IS_DRY_RUN";

fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    let path =
        env::var(FEED_CONFIG_PATH_ENV).expect(&format!("{} must be set", FEED_CONFIG_PATH_ENV));
    let file = File::open(path)?;
    let config: Config = serde_yaml::from_reader(file)?;
    Ok(config)
}

async fn setup_connection() -> Result<DatabaseConnection, DbErr> {
    let database_url =
        env::var(DATABASE_URL_ENV).expect(&format!("{} must be set", DATABASE_URL_ENV));
    Database::connect(database_url).await
}

async fn setup_tables(db: &DatabaseConnection) -> Result<(), DbErr> {
    let backend = db.get_database_backend();
    let schema = Schema::new(backend);
    let schema_manager = SchemaManager::new(db);
    schema_manager
        .create_table(
            schema
                .create_table_from_entity(PostedItem)
                .if_not_exists()
                .take(),
        )
        .await?;
    for mut stmt in schema.create_index_from_entity(PostedItem) {
        schema_manager
            .create_index(stmt.if_not_exists().take())
            .await?;
    }
    Ok(())
}

async fn fetch_feed(url: &str) -> Result<Channel, reqwest::Error> {
    let content = reqwest::get(url).await?.text().await?;
    let channel = content.parse::<Channel>().unwrap();
    Ok(channel)
}

async fn process_feed(
    feed: &Feed,
    base_url: &String,
    is_dry_run: &bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut updater = AverageUpdater::new();
    let client = megalodon::generator(
        megalodon::SNS::Mastodon,
        base_url.clone(),
        Some(feed.token.clone()),
        None,
    );
    let db = setup_connection().await?;
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
                updater.update(before.pub_date_utc());
            }
        };
        // タイトルを取得して以前と同じなら待機
        let title = item.title().unwrap_or_default().to_string();
        if updater.last_title().as_ref() == Some(&title) {
            sleep(updater.get_next_wait()).await;
            continue;
        }
        updater.set_last_title(Some(title));
        let status = item.to_status();
        let pub_date = item.pub_date_utc();
        println!("{} -> \n{}", feed.url, status);
        if !*is_dry_run {
            let res = client.post_status(status, Some(
                &PostStatusInputOptions {
                    // テスト垢投稿用
                    // visibility: Some(megalodon::entities::status::StatusVisibility::Unlisted),
                    ..PostStatusInputOptions::default()
                }
            )).await?;
            let PostStatusOutput::Status(status) = res.json() else {
                panic!("unexpected response");
            };
            posted_item::ActiveModel {
                source: Set(feed.url.clone()),
                title: Set(item.title().unwrap().to_string()),
                link: Set(item.link().unwrap().to_string()),
                pub_date: Set(pub_date),
                post_id: Set(status.id),
                ..Default::default()
            }.insert(&db).await?;
        }
        updater.update(pub_date);
        sleep(updater.get_next_wait()).await;
    }
}

async fn sleep(duration: Duration) {
    println!("sleep {}", duration.to_readable_string());
    tokio::time::sleep(duration.to_std().unwrap()).await;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config()?;
    let db = setup_connection().await?;
    let is_dry_run = env::var(IS_DRY_RUN_ENV).is_ok();
    setup_tables(&db).await?;

    let tasks: Vec<_> = config
        .feeds
        .iter()
        .map(|rss| process_feed(rss, &config.base_url, &is_dry_run))
        .collect();

    futures::future::join_all(tasks).await;
    Ok(())
}
