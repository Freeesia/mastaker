use crate::ext_trait::*;
use chrono::Duration;

pub async fn sleep(duration: &Duration, reason: &str) {
    println!("{} sleep {}", reason, duration.to_iso8601());
    #[cfg(feature = "skip_sleep")]
    tokio::time::sleep(
        match duration {
            d if d > Duration::minutes(1) => Duration::seconds(10),
            _ => duration,
        }
        .to_std()
        .unwrap(),
    )
    .await;
    #[cfg(not(feature = "skip_sleep"))]
    tokio::time::sleep(duration.to_std().unwrap()).await;
}
