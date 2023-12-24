use chrono::Duration;
use once_cell::sync::Lazy;
use std::env;

pub const DATABASE_URL_ENV: &str = "DATABASE_URL";
pub const FEED_CONFIG_PATH_ENV: &str = "FEED_CONFIG_PATH";
pub const IS_DRY_RUN_ENV: &str = "IS_DRY_RUN";

pub static QUEUE_INTERVAL: Lazy<Duration> = Lazy::new(|| {
    Duration::seconds(
        env::var("QUEUE_INTERVAL")
            .unwrap_or("1".to_string())
            .parse()
            .unwrap(),
    )
});
pub static POST_INTERVAL: Lazy<Duration> = Lazy::new(|| {
    Duration::seconds(
        env::var("POST_INTERVAL")
            .unwrap_or("5".to_string())
            .parse()
            .unwrap(),
    )
});
pub static MAX_QUEUE: Lazy<usize> = Lazy::new(|| {
    env::var("MAX_QUEUE")
        .unwrap_or("1000".to_string())
        .parse()
        .unwrap()
});
pub static MIN_WAIT: Lazy<Duration> = Lazy::new(|| {
    Duration::minutes(
        env::var("MIN_WAIT")
            .unwrap_or("5".to_string())
            .parse()
            .unwrap(),
    )
});
pub static MAX_WAIT: Lazy<Duration> = Lazy::new(|| {
    Duration::minutes(
        env::var("MAX_WAIT")
            .unwrap_or("60".to_string())
            .parse()
            .unwrap(),
    )
});
pub static CONFIG_INTERVAL: Lazy<Duration> = Lazy::new(|| {
    Duration::seconds(
        env::var("CONFIG_INTERVAL")
            .unwrap_or("60".to_string())
            .parse()
            .unwrap(),
    )
});