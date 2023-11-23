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
use std::{collections::HashMap, env};
use tokio::sync::mpsc::*;

use constants::*;
use ext_trait::*;
use schema::*;
use setup::*;
use utility::*;

async fn feed_loop(config: &FeedConfig, tx: Sender<PostInfo>) -> anyhow::Result<()> {
    let db = setup_connection().await?;
    let info = FeedInfo::find_by_id(&config.id)
        .one(&db)
        .await?
        .unwrap_or(feed_info::Model::new(config.id.clone()));
    match info.next_fetch {
        date if date == DateTimeUtc::UNIX_EPOCH => {
            sleep(
                Duration::seconds(rand::thread_rng().gen_range(10..=60)),
                &config.id,
            )
            .await;
        }
        next => {
            let now = Utc::now();
            if next > now {
                sleep(next - now, &config.id).await;
            } else {
                sleep(
                    Duration::seconds(rand::thread_rng().gen_range(10..=60)),
                    &config.id,
                )
                .await;
            }
        }
    }
    loop {
        if let Err(err) = process_feed(&db, config, &tx).await {
            let id = capture_anyhow(&err);
            println!("failed to process feed: {:?}, sentry: {}", err, id);
            sleep(Duration::minutes(20), &config.id).await;
        };
    }
}

async fn process_feed(
    db: &DatabaseConnection,
    config: &FeedConfig,
    tx: &Sender<PostInfo>,
) -> anyhow::Result<()> {
    println!("check feed: {}", config.id);
    let mut info = match FeedInfo::find_by_id(&config.id).one(db).await? {
        Some(info) => info.into_active_model(),
        None => {
            let info = feed_info::Model::new(config.id.clone()).into_active_model();
            info.insert(db).await?.into_active_model()
        }
    };
    let content = reqwest::get(&config.url).await?.bytes().await?;
    let feed = FeedParser::parse_with_uri(content.as_ref(), Some(&config.url))?;
    // 最新の投稿を取得
    let Some(entry) = feed
        .entries
        .iter()
        .max_by_key(|e| e.pub_date_utc().unwrap())
    else {
        // 1番目の記事が存在しない場合は待機
        let d = info.update_next_fetch(&feed);
        info.save(db).await?;
        sleep(d, &config.id).await;
        return Ok(());
    };

    // キューに追加済みの最新の投稿を取得
    let last_post = PostItem::find()
        .filter(post_item::Column::Source.eq(&config.id))
        .order_by_desc(post_item::Column::PubDate)
        .one(db)
        .await?;

    // 初回は投稿せずに登録のみ
    let Some(last_post) = last_post else {
        insert_post(db, &config.id, entry).await?;
        let d = info.update_next_fetch(&feed);
        info.save(db).await?;
        sleep(d, &config.id).await;
        return Ok(());
    };

    if feed.entries.iter().any(|e| e.published == None) {
        // atom 0.3 は published がないので、last_posted と比較する
        let entry = feed.entries.get(0).unwrap();
        let title = &entry.title.as_ref().unwrap().content;
        let link = &entry.links.get(0).unwrap().href;
        if last_post.title != *title || last_post.link != *link {
            let post = insert_post(db, &config.id, entry).await?;
            tx.send(PostInfo(post.id, entry.clone(), config.clone()))
                .await?;
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
            let post = insert_post(db, &config.id, entry).await?;
            tx.send(PostInfo(post.id, entry.clone(), config.clone()))
                .await?;
        }
    }

    let d = info.update_next_fetch(&feed);
    info.update(db).await?;
    sleep(d, &config.id).await;
    Ok(())
}

async fn insert_post(
    db: &DatabaseConnection,
    source: &String,
    entry: &Entry,
) -> Result<post_item::Model, anyhow::Error> {
    let post = post_item::ActiveModel {
        source: Set(source.to_owned()),
        title: Set(entry.title.as_ref().unwrap().content.to_owned()),
        link: Set(entry.links.get(0).unwrap().href.clone()),
        pub_date: Set(*entry.pub_date_utc_or(&Utc::now())),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(post)
}

struct PostInfo(i32, Entry, FeedConfig);

async fn post_loop(mut rx: Receiver<PostInfo>, base_url: &String, is_dry_run: &bool) {
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
        let posted_id = post(&client, &config, &entry, is_dry_run).await;
        if let Err(e) = posted_id {
            let id = capture_anyhow(&e);
            println!("failed to post: {:?}, sentry: {}", e, id);
            sleep(Duration::seconds(10), &config.id).await;
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

        sleep(Duration::seconds(5), &config.id).await;
    }
}

async fn post(
    client: &Box<dyn Megalodon + Send + Sync>,
    config: &FeedConfig,
    entry: &Entry,
    is_dry_run: &bool,
) -> anyhow::Result<String> {
    let status = entry.to_status(config.id.clone(), &config.tag).await?;
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

fn main() {
    let _guard = sentry::init(sentry::ClientOptions {
        release: sentry::release_name!(),
        #[cfg(debug_assertions)]
        debug: true,
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
    let db = setup_connection().await?;
    let is_dry_run = env::var(IS_DRY_RUN_ENV).is_ok();
    setup_tables(&db).await?;

    let (tx, rx) = channel(100);

    let tasks: Vec<_> = config
        .feeds
        .iter()
        .map(|rss| feed_loop(rss, tx.clone()))
        .collect();

    let task = tokio::spawn(async move {
        post_loop(rx, &config.base_url, &is_dry_run).await;
    });

    _ = futures::future::join(futures::future::join_all(tasks), task).await;
    Ok(())
}
