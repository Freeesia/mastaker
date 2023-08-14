use chrono::{DateTime, Duration, Utc};
use getset::{Getters, Setters};

// 最後の更新からの平均時間を保持するための構造体
#[derive(Getters, Setters)]
pub struct AverageUpdater {
    #[getset(get = "pub")]
    last_time: Option<DateTime<Utc>>,
    current_average: Option<Duration>,
    #[getset(set = "pub", get = "pub")]
    last_title: Option<String>,
}

impl AverageUpdater {
    pub fn new() -> Self {
        AverageUpdater {
            last_time: None,
            current_average: None,
            last_title: None,
        }
    }

    // 新しい更新の時間を基に平均更新間隔を更新
    pub fn update_from_rfc2822(&mut self, new_time: &str) {
        self.update(
            DateTime::parse_from_rfc2822(new_time)
                .unwrap()
                .with_timezone(&Utc),
        );
    }

    // 新しい更新の時間を基に平均更新間隔を更新
    pub fn update(&mut self, new_time: DateTime<Utc>) {
        if let Some(last) = self.last_time {
            let diff = new_time - last;
            self.current_average = Some(if let Some(avg) = self.current_average {
                (avg + diff) / 2
            } else {
                diff
            });
        }
        self.last_time = Some(new_time);
    }

    // 次の待ち時間を取得
    pub fn get_next_wait(&self) -> chrono::Duration {
        let minimum = Duration::minutes(5);
        let maximum = Duration::hours(12);

        // 現在の平均が存在しない場合は最小待ち時間を返す
        self.current_average
            .unwrap_or(maximum)
            .clamp(minimum, maximum)
    }
}
