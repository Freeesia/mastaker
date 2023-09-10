mod ext_trait;
mod feed_info;
mod posted_item;

extern crate rand;

use chrono::{Duration, Utc};
use feed_info::Entity as FeedInfo;
use feed_rs::{model::Entry, parser as FeedParser};
use megalodon::{megalodon::PostStatusOutput, Megalodon};
use posted_item::Entity as PostedItem;
use rand::Rng;
use reqwest;
use sea_orm::{prelude::DateTimeUtc, *};
use sea_orm_migration::SchemaManager;
use serde_derive::{Deserialize, Serialize};
use serde_yaml;
use std::{env, fs::File};

use crate::ext_trait::*;

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    base_url: String,
    feeds: Vec<FeedConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FeedConfig {
    id: String,
    url: String,
    token: String,
    tag: Option<TagConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TagConfig {
    always: Vec<String>,
    ignore: Vec<String>,
    replace: Vec<String>,
    xpath: Option<String>,
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
    schema_manager
        .create_table(
            schema
                .create_table_from_entity(FeedInfo)
                .if_not_exists()
                .take(),
        )
        .await?;
    for mut stmt in schema.create_index_from_entity(PostedItem) {
        schema_manager
            .create_index(stmt.if_not_exists().take())
            .await?;
    }
    for mut stmt in schema.create_index_from_entity(FeedInfo) {
        schema_manager
            .create_index(stmt.if_not_exists().take())
            .await?;
    }
    Ok(())
}

async fn fetch_feed(url: &str) -> Result<feed_rs::model::Feed, Box<dyn std::error::Error>> {
    let content = reqwest::get(url).await?.bytes().await?;
    let feed =
        FeedParser::parse_with_uri(content.as_ref(), Some(url)).expect("failed to parse rss");
    Ok(feed)
}

async fn process_feed(
    config: &FeedConfig,
    base_url: &String,
    is_dry_run: &bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = megalodon::generator(
        megalodon::SNS::Mastodon,
        base_url.clone(),
        Some(config.token.clone()),
        None,
    );
    let db = setup_connection().await?;
    let info = FeedInfo::find_by_id(&config.url)
        .one(&db)
        .await?
        .unwrap_or(feed_info::Model::new(config.url.clone()));
    match info.next_fetch {
        date if date == DateTimeUtc::MIN_UTC => {
            sleep(
                Duration::seconds(rand::thread_rng().gen_range(10..=60)),
                &config.url,
            )
            .await;
        }
        next => {
            let now = Utc::now();
            if next > now {
                sleep(next - now, &config.url).await;
            } else {
                sleep(
                    Duration::seconds(rand::thread_rng().gen_range(10..=60)),
                    &config.url,
                )
                .await;
            }
        }
    }
    loop {
        let mut info = FeedInfo::find_by_id(&config.url)
            .one(&db)
            .await?
            .unwrap_or(feed_info::Model::new(config.url.clone()));
        let feed = fetch_feed(&config.url).await?;
        // 1番目の記事が存在しない場合は待機
        let Some(entry) = feed.entries.get(0) else{
            let d = info.update_next_fetch(&feed, false);
            info.into_active_model().save(&db).await?;
            sleep(d, &config.url).await;
            continue;
        };

        if info.last_post == 0 {
            // 初回は投稿せずに登録のみ
            info.last_post = register(&db, &config.id, entry, &"".to_string()).await?;
            let d = info.update_next_fetch(&feed, true);
            info.into_active_model().insert(&db).await?;
            sleep(d, &config.url).await;
            continue;
        };

        let last_posted = PostedItem::find_by_id(info.last_post)
            .one(&db)
            .await?
            .unwrap();
        let mut posted = false;
        if feed.entries.iter().any(|e| e.published == None) {
            // atom 0.3 は published がないので、last_posted と比較する
            let entry = feed.entries.get(0).unwrap();
            let title = &entry.title.as_ref().unwrap().content;
            let link = &entry.links.get(0).unwrap().href;
            if last_posted.title != *title || last_posted.link != *link {
                let posted_id = post(&client, &config, entry, is_dry_run).await?;
                info.last_post = register(&db, &config.id, entry, &posted_id).await?;
                posted = true;
            }
        } else {
            let entries = feed
                .entries
                .iter()
                .rev()
                .skip_while(|e| e.pub_date_utc().unwrap() <= last_posted.pub_date);
            for entry in entries {
                let posted_id = post(&client, &config, entry, is_dry_run).await?;
                info.last_post = register(&db, &config.id, entry, &posted_id).await?;
                posted = true;
                sleep(Duration::seconds(30), &config.url).await;
            }
        }

        let d = info.update_next_fetch(&feed, posted);
        info.into_active_model().save(&db).await?;
        sleep(d, &config.url).await;
    }
}

async fn post(
    client: &Box<dyn Megalodon + Send + Sync>,
    config: &FeedConfig,
    entry: &Entry,
    is_dry_run: &bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let status = entry.to_status(config.id.clone(), &config.tag).await?;
    let pud_date = entry.pub_date_utc_or(Utc::now());
    println!(
        "source: {}, pub: {} -> \n{}",
        config.url,
        pud_date.to_rfc3339(),
        status
    );
    let mut posted_id = "".to_string();
    if !*is_dry_run {
        let res = client.post_status(status, None).await?;
        let PostStatusOutput::Status(status) = res.json() else {
            return Err("failed to post".into());
        };
        posted_id = status.id;
    }
    Ok(posted_id)
}

async fn register(
    db: &DatabaseConnection,
    source: &String,
    entry: &Entry,
    posted_id: &String,
) -> Result<i32, Box<dyn std::error::Error>> {
    let posted = posted_item::ActiveModel {
        source: Set(source.clone()),
        title: Set(entry.title.clone().unwrap().content),
        link: Set(entry.links.get(0).unwrap().href.clone()),
        pub_date: Set(entry.pub_date_utc_or(Utc::now())),
        post_id: Set(posted_id.clone()),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(posted.id)
}

async fn sleep(duration: Duration, source: &str) {
    println!("{} sleep {}", source, duration.to_readable_string());
    #[cfg(debug_assertions)]
    tokio::time::sleep(
        match duration {
            d if d > Duration::minutes(1) => Duration::seconds(10),
            _ => duration,
        }
        .to_std()
        .unwrap(),
    )
    .await;
    #[cfg(not(debug_assertions))]
    tokio::time::sleep(duration.to_std().unwrap()).await;
}

fn main() {
    let _guard = sentry::init(sentry::ClientOptions {
        release: sentry::release_name!(),
        #[cfg(debug_assertions)]
        debug: true,
        ..Default::default()
    });
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(run())
        .unwrap();
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
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
