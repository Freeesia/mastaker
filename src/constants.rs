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
