use crate::ext_trait::*;
use chrono::Duration;

pub async fn sleep(duration: Duration, source: &str) {
    println!("{} sleep {}", source, duration.to_iso8601());
    #[cfg(skip_sleep)]
    tokio::time::sleep(
        match duration {
            d if d > Duration::minutes(1) => Duration::seconds(10),
            _ => duration,
        }
        .to_std()
        .unwrap(),
    )
    .await;
    #[cfg(not(skip_sleep))]
    tokio::time::sleep(duration.to_std().unwrap()).await;
}
