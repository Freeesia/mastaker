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
            let second = feed.entries.get(1).unwrap().pub_date_utc_or(&default);
            ttl.min(*first - *second).max(Duration::minutes(5))
        } else {
            ttl.max(Duration::minutes(5))
        }
    }

    /// 基本的には更新間隔の中央値の1/6を使用します。
    /// 前回のチェックから1回投稿があれば、前回の値を使用します。
    /// 前回のチェックから2回以上投稿があれば、半分の値を使用します。
    /// 前回のチェックから1回も投稿がなければ、前回の1.1倍の値を使用します。(中央値の1/6を超えない)
    /// 中央値を超えるまでに1回も投稿がなければ、それ以降から前回の1.1倍の値を使用します。
    fn get_next_duration(feed: &Feed, last_fetch: &DateTimeUtc, is_posted: bool) -> Duration {
        let mut tmp = feed.entries.clone();
        tmp.sort_by_key(|e| *e.pub_date_utc().unwrap());
        let mut durations = Vec::with_capacity(tmp.len() - 1);
        for (prev, next) in tmp.iter().zip(tmp.iter().skip(1)) {
            durations.push(*next.pub_date_utc().unwrap() - *prev.pub_date_utc().unwrap());
        }
        // 5分未満は連続投稿扱いで無視
        durations = durations
            .into_iter()
            .filter(|d| *d > Duration::minutes(5))
            .collect();
        // 中央値の算出
        let median = median6(durations);

    }
}

fn median6(mut durations: Vec<Duration>) -> Duration {
    // 中央値の算出
    durations.sort();
    let len = durations.len();
    let median = if len % 2 == 0 {
        (durations[len / 2 - 1] + durations[len / 2]) / 2
    } else {
        durations[len / 2]
    };
    // 中央値の1/6を算出
    median / 6
}
