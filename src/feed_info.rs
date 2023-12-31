use chrono::{Duration, Utc};
use feed_rs::model::Feed;
use sea_orm::entity::prelude::*;
use sea_orm::ActiveValue::*;

use crate::constants::{MIN_WAIT, MAX_WAIT};
use crate::ext_trait::ItemExt;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "feed_info")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub source: String,
    pub last_fetch: DateTimeUtc,
    pub next_fetch: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    pub fn new(source: String) -> Self {
        Self {
            source,
            last_fetch: DateTimeUtc::UNIX_EPOCH,
            next_fetch: DateTimeUtc::UNIX_EPOCH,
        }
    }
}

impl ActiveModel {
    pub fn update_next_fetch(&mut self, feed: &Feed) -> Duration {
        if self.last_fetch.as_ref() == &DateTimeUtc::UNIX_EPOCH {
            let duration = Self::get_first_duration(feed);
            self.last_fetch = Set(Utc::now());
            self.next_fetch = Set(*self.last_fetch.as_ref() + duration);
            duration
        } else {
            let duration = Self::get_next_duration(feed, self.last_fetch.as_ref())
                .max(*MIN_WAIT)
                .min(*MAX_WAIT);
            self.last_fetch = Set(Utc::now());
            self.next_fetch = Set(*self.last_fetch.as_ref() + duration);
            duration
        }
    }

    /// 最初のフィードの場合、現在の時間を使用します。
    /// フィードの時間寿命（TTL）と最初の2つのエントリーの時間差に基づいて、フィードの期間を計算します。
    /// エントリーが2つ未満の場合、TTL / 6 が使用されます。
    /// 5分未満の場合、5分を使用します。
    fn get_first_duration(feed: &Feed) -> Duration {
        let now = Utc::now();
        let ttl = Duration::minutes(feed.ttl.unwrap_or(60) as i64);
        let mut pubs = feed
            .entries
            .iter()
            .map(|e| e.pub_date_utc_or(&now))
            .collect::<Vec<_>>();
        pubs.sort();
        pubs.dedup();
        if pubs.len() > 2 {
            let first = pubs.get(0).unwrap();
            let second = pubs.get(1).unwrap();
            MIN_WAIT.max(ttl.min(**second - **first) / 6)
        } else {
            MIN_WAIT.max(ttl / 6)
        }
    }

    /// 前回のチェックから2回以上投稿があれば、前回のチェック間隔から半分の値を使用します。
    /// 前回のチェックから1回投稿があれば、前回のチェック間隔を使用します。
    /// 前回のチェックから1回も投稿がないかつ間隔が中央値の1/6未満なら、中央値の1/6を使用します。
    /// 前回のチェックから1回も投稿がないかつ間隔が中央値未満なら、前回の1.1倍の値を使用します。(中央値を超えない)
    /// 中央値を超えるまでに1回も投稿がなければ、それ以降から前回の1.5倍の値を使用します。
    fn get_next_duration(feed: &Feed, last_fetch: &DateTimeUtc) -> Duration {
        // 前回のチェックから現在時刻の間隔の取得
        let duration = Utc::now() - *last_fetch;

        // そもそも1回も投稿がなければ、前回のチェック間隔から1.5倍の値を使用
        if feed.entries.is_empty() {
            return duration * 3 / 2;
        }

        // 前回のチェックからの投稿を取得
        let mut last_posted: Vec<_> = feed
            .entries
            .iter()
            .filter(|e| e.pub_date_utc().unwrap() > last_fetch)
            .collect();
        last_posted.sort_by_key(|e| e.pub_date_utc().unwrap());
        // 前回のチェックから2回以上投稿があれば、半分の値を使用
        if last_posted.len() >= 2 {
            return duration / 2;
        }
        // 前回のチェックから1回投稿があれば、前回の投稿からの同じ間隔を使用
        if last_posted.len() == 1 {
            return *last_posted[0].pub_date_utc().unwrap() - *last_fetch;
        }

        // 前回のチェックから1回も投稿がなければ、
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
        // 1回も投稿がなければ、前回のチェック間隔から1.5倍の値を使用
        if durations.is_empty() {
            return duration * 3 / 2;
        }
        let median = median(durations);
        let median6 = median / 6;
        if duration < median6 {
            return median6;
        } else if duration < median {
            return duration * 11 / 10;
        } else {
            return duration * 3 / 2;
        }
    }
}

fn median(mut durations: Vec<Duration>) -> Duration {
    // 中央値の算出
    durations.sort();
    let len = durations.len();
    if len % 2 == 0 {
        (durations[len / 2 - 1] + durations[len / 2]) / 2
    } else {
        durations[len / 2]
    }
}
