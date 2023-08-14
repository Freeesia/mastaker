use chrono::Duration;

pub trait ReadableString {
    fn to_readable_string(&self) -> String;
}

impl ReadableString for Duration {
    fn to_readable_string(&self) -> String {
        let total_seconds = self.num_seconds();
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;
        let mut components = vec![];

        if hours > 0 {
            components.push(format!("{}時間", hours));
        }
        if minutes > 0 {
            components.push(format!("{}分", minutes));
        }
        if seconds > 0 || components.is_empty() {
            components.push(format!("{}秒", seconds));
        }

        components.join("")
    }
}
