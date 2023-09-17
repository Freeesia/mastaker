use chrono::{Duration, Utc};
use feed_rs::model::Feed;
use sea_orm::entity::prelude::*;
use sea_orm::ActiveValue::*;

use crate::ext_trait::ItemExt;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "feed_info")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub source: String,
    pub last_fetch: DateTimeUtc,
    pub next_fetch: DateTimeUtc,
    pub last_post: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    pub fn new(source: String) -> Self {
        Self {
            source,
            last_fetch: DateTimeUtc::MIN_UTC,
            next_fetch: DateTimeUtc::MIN_UTC,
            last_post: 0,
        }
    }
}

impl ActiveModel {
    pub fn update_next_fetch(&mut self, feed: &Feed, is_posted: bool) -> Duration {
        if self.last_fetch.as_ref() == &DateTimeUtc::MIN_UTC {
            let duration = Self::get_first_duration(feed);
            self.last_fetch = Set(Utc::now());
            self.next_fetch = Set(*self.last_fetch.as_ref() + duration);
            duration
        } else {
            let duration = Self::get_next_duration(feed, self.last_fetch.as_ref(), is_posted)
                .max(Duration::minutes(5))
                .min(Duration::minutes(feed.ttl.unwrap_or(60) as i64));
            self.last_fetch = Set(Utc::now());
            self.next_fetch = Set(*self.last_fetch.as_ref() + duration);
            duration
        }
    }

    /// 最初のフィードの場合、現在の時間を使用します。
    /// フィードの時間寿命（TTL）と最初の2つのエントリーの時間差に基づいて、フィードの期間を計算します。
    /// エントリーが2つ未満の場合、TTLが使用されます。
    /// 5分未満の場合、5分を使用します。
    fn get_first_duration(feed: &Feed) -> Duration {
        let now = Utc::now();
        let ttl = Duration::minutes(feed.ttl.unwrap_or(60) as i64);
        if feed.entries.len() > 2 {
            let default = now - Duration::hours(1);
            let first = feed.entries.get(0).unwrap().pub_date_utc_or(&now);
            let second = feed
                .entries
                .get(1)
                .unwrap()
                .pub_date_utc_or(&default);
            ttl.min(*first - *second).max(Duration::minutes(5))
        } else {
            ttl.max(Duration::minutes(5))
        }
    }

    fn get_next_duration(feed: &Feed, last_fetch: &DateTimeUtc, is_posted: bool) -> Duration {
        let now = Utc::now();

        // 最後のフィードが存在しない場合、前回からの経過時間+1時間を使用します。
        let Some(last_entry) = feed.entries.get(0) else {
            let druation = now - last_fetch + Duration::hours(1);
            return druation;
        };

        if is_posted {
            // 前回の更新から投稿があった場合、経過時間/2
            let duration = (now - last_entry.pub_date_utc_or(last_fetch)) / 2;
            return duration;
        }

        // 前回の更新から投稿がなかった場合、経過時間*1.5
        // 増加分は最大1時間
        let duration = now - last_entry.pub_date_utc_or(last_fetch);
        let increment = Duration::hours(1).min(duration / 2);
        duration + increment
    }
}
