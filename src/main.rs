mod constants;
mod ext_trait;
mod feed_info;
mod post_item;
mod schema;
mod setup;
mod utility;

extern crate rand;

use chrono::{Duration, Utc};
use feed_info::Entity as FeedInfo;
use feed_rs::{model::Entry, parser as FeedParser};
use megalodon::{megalodon::PostStatusOutput, Megalodon};
use post_item::Entity as PostItem;
use rand::Rng;
use reqwest;
use sea_orm::{prelude::DateTimeUtc, *};
use sentry_anyhow::capture_anyhow;
use std::{
    collections::{HashMap, HashSet},
    env,
};
use tokio::sync::mpsc::*;
use tokio_retry::{strategy::FixedInterval, RetryIf};

use constants::*;
use ext_trait::*;
use schema::*;
use setup::*;
use utility::*;

async fn feed_loop(config: &FeedConfig, tx: Sender<PostInfo>) -> anyhow::Result<()> {
    let info = {
        let db = setup_connection().await?;
        FeedInfo::find_by_id(&config.id)
            .one(&db)
            .await?
            .unwrap_or(feed_info::Model::new(config.id.clone()))
    };
    match info.next_fetch {
        date if date == DateTimeUtc::UNIX_EPOCH => {
            let time = Duration::seconds(rand::thread_rng().gen_range(10..=60));
            sleep(&time, &format!("init rand wait: {}", config.id)).await;
        }
        next => {
            let now = Utc::now();
            if next > now {
                sleep(&(next - now), &format!("init wait: {}", config.id)).await;
            } else {
                let time = Duration::seconds(rand::thread_rng().gen_range(10..=60));
                sleep(&time, &format!("init rand wait: {}", config.id)).await;
            }
        }
    }
    loop {
        if let Err(err) = process_feed(config, &tx).await {
            let id = capture_anyhow(&err);
            println!("failed to process feed: {:?}, sentry: {}", err, id);
            sleep(
                &Duration::minutes(20),
                &format!("faild wait: {}", config.id),
            )
            .await;
        };
    }
}

async fn process_feed(config: &FeedConfig, tx: &Sender<PostInfo>) -> anyhow::Result<()> {
    println!("check feed: {}", config.id);
    let db = setup_connection().await?;
    let mut info = match FeedInfo::find_by_id(&config.id).one(&db).await? {
        Some(info) => info.into_active_model(),
        None => {
            let info = feed_info::Model::new(config.id.clone()).into_active_model();
            info.insert(&db).await?.into_active_model()
        }
    };
    let content = reqwest::get(&config.url).await?.bytes().await?;
    let feed = FeedParser::parse_with_uri(content.as_ref(), Some(&config.url))?;
    // 最新の投稿を取得
    let Some(entry) = feed
        .entries
        .iter()
        .filter(|e| e.title.is_some() && e.links.len() > 0)
        .max_by_key(|e| e.pub_date_utc().unwrap())
    else {
        // 1番目の記事が存在しない場合は待機
        let d = info.update_next_fetch(&feed);
        info.save(&db).await?;
        sleep(&d, &format!("not found: {}", config.id)).await;
        return Ok(());
    };

    // キューに追加済みの最新の投稿を取得
    let last_post = PostItem::find()
        .filter(post_item::Column::Source.eq(&config.id))
        .order_by_desc(post_item::Column::PubDate)
        .one(&db)
        .await?;

    // 初回は投稿せずに登録のみ
    let Some(last_post) = last_post else {
        PostItem::insert(&db, &config.id, entry).await?;
        let d = info.update_next_fetch(&feed);
        info.save(&db).await?;
        sleep(&d, &format!("first wait: {}", config.id)).await;
        return Ok(());
    };

    if feed.entries.iter().any(|e| e.published == None) {
        // atom 0.3 は published がないので、last_posted と比較する
        let entry = feed.entries.get(0).unwrap();
        let title = &entry.title.as_ref().unwrap().content;
        let link = &entry.links.get(0).unwrap().href;
        if last_post.title != *title || last_post.link != *link {
            let post = PostItem::insert(&db, &config.id, entry).await?;
            tx.send(PostInfo(post.id, entry.clone(), config.clone()))
                .await?;
            sleep(&QUEUE_INTERVAL, &format!("queue wait : {}", config.id)).await;
        }
    } else {
        // 前回投稿日時以降の記事を投稿する
        let mut entries: Vec<_> = feed
            .entries
            .iter()
            .filter(|e| e.pub_date_utc().unwrap() > &last_post.pub_date)
            .collect();
        // 公開日時でソートする
        entries.sort_by_key(|e| e.pub_date_utc().unwrap());
        for entry in entries {
            let post = PostItem::insert(&db, &config.id, entry).await?;
            tx.send(PostInfo(post.id, entry.clone(), config.clone()))
                .await?;
            sleep(&QUEUE_INTERVAL, &format!("queue wait : {}", config.id)).await;
        }
    }

    let d = info.update_next_fetch(&feed);
    info.update(&db).await?;
    db.close().await?;
    sleep(&d, &format!("check wait: {}", config.id)).await;
    Ok(())
}

struct PostInfo(i32, Entry, FeedConfig);

async fn post_loop(
    mut rx: Receiver<PostInfo>,
    base_url: &String,
    tag: &Option<TagConfig>,
    is_dry_run: &bool,
) {
    let db = setup_connection().await.unwrap();
    let mut cache = HashMap::new();
    while let Some(PostInfo(id, entry, config)) = rx.recv().await {
        println!("Got: {:?}", entry);
        let client = cache.entry(config.url.clone()).or_insert_with(|| {
            megalodon::generator(
                megalodon::SNS::Mastodon,
                base_url.clone(),
                Some(config.token.clone()),
                None,
            )
        });
        let posted_id = RetryIf::spawn(
            FixedInterval::from_millis(5000).take(2),
            || async { post(&client, &config, &tag, &entry, is_dry_run).await },
            |e: &anyhow::Error| {
                if let Some(e) = e.downcast_ref::<megalodon::error::Error>() {
                    if let megalodon::error::Error::OwnError(e) = e {
                        if let Some(s) = e.status {
                            if s == 429 {
                                println!("retry: {}", config.id);
                                return true;
                            }
                        }
                    }
                }
                false
            },
        )
        .await;
        if let Err(e) = posted_id {
            let id = capture_anyhow(&e);
            println!("failed to post: {:?}, sentry: {}", e, id);
            sleep(
                &Duration::seconds(10),
                &format!("failed post: {}", config.id),
            )
            .await;
            continue;
        }
        let posted_id = posted_id.unwrap();
        if let Err(e) = PostItem::update(post_item::ActiveModel {
            id: Set(id),
            post_id: Set(Some(posted_id)),
            ..Default::default()
        })
        .exec(&db)
        .await
        {
            let id = capture_anyhow(&anyhow::anyhow!(format!("failed: {:?}", e)));
            println!("failed to update post id: {:?}, sentry: {}", e, id);
        }

        if let Ok(queue_count) = PostItem::find()
            .filter(post_item::Column::PostId.is_null())
            .count(&db)
            .await
        {
            if let Ok(feed_count) = FeedInfo::find().count(&db).await {
                println!("queue count: {}", queue_count - feed_count);
            } else {
                println!("failed to count feed");
            }
        } else {
            println!("failed to count queue");
        }
        sleep(&POST_INTERVAL, &format!("post wait: {}", config.id)).await;
    }
}

async fn post(
    client: &Box<dyn Megalodon + Send + Sync>,
    config: &FeedConfig,
    global_tag: &Option<TagConfig>,
    entry: &Entry,
    is_dry_run: &bool,
) -> anyhow::Result<String> {
    let mut merged_tag = TagConfig::new();
    if let Some(tag) = global_tag {
        merged_tag.always.extend(tag.always.clone());
        merged_tag.ignore.extend(tag.ignore.clone());
        merged_tag.replace.extend(tag.replace.clone());
        merged_tag.xpath = tag.xpath.clone();
    }
    if let Some(tag) = &config.tag {
        merged_tag.always.extend(tag.always.clone());
        merged_tag.ignore.extend(tag.ignore.clone());
        merged_tag.replace.extend(tag.replace.clone());
        merged_tag.xpath = tag.xpath.clone();
    }
    let status = entry.to_status(config.id.clone(), &merged_tag).await?;
    let now = Utc::now();
    let pud_date = entry.pub_date_utc_or(&now);
    println!(
        "source: {}, pub: {} rag: {}",
        config.id,
        pud_date.to_rfc3339(),
        (now - pud_date).to_iso8601()
    );
    if *is_dry_run {
        println!("dry run");
        Ok("".to_string())
    } else {
        let res = client.post_status(status, None).await?;
        let PostStatusOutput::Status(status) = res.json() else {
            return Err(anyhow::anyhow!(format!(
                "failed expected response: {:?}",
                res
            )));
        };
        Ok(status.id)
    }
}

async fn config_reload_loop(tx: Sender<PostInfo>) {
    let mut feeds = HashSet::new();
    loop {
        match load_config() {
            Ok(config) => {
                println!("config reloaded");
                for feed in config.feeds {
                    if !feeds.insert(feed.id.clone()) {
                        continue;
                    }
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        _ = feed_loop(&feed, tx).await;
                    });
                }
            }
            Err(e) => {
                println!("failed to load config: {:?}", e);
            }
        }
        sleep(&CONFIG_INTERVAL, "config wait").await;
    }
}

fn main() {
    let _guard = sentry::init(sentry::ClientOptions {
        release: sentry::release_name!(),
        #[cfg(debug_assertions)]
        debug: true,
        attach_stacktrace: true,
        ..Default::default()
    });
    // sentryを動かすために必要
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(run())
        .unwrap();
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config()?;
    let is_dry_run = env::var(IS_DRY_RUN_ENV).is_ok();
    setup_tables().await?;

    let (tx, rx) = channel(*MAX_QUEUE);

    _ = tokio::join!(
        post_loop(rx, &config.base_url, &config.tag, &is_dry_run),
        config_reload_loop(tx)
    );
    Ok(())
}
