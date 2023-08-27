use chrono::{Duration, DateTime, Utc};

pub trait ReadableString {
    fn to_readable_string(&self) -> String;
}

impl ReadableString for Duration {
    fn to_readable_string(&self) -> String {
        let total_seconds = self.num_seconds();
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;
        let mut builder = string_builder::Builder::default();

        if hours > 0 {
            builder.append(format!("{}時間", hours));
        }
        if minutes > 0 {
            builder.append(format!("{}分", minutes));
        }
        if seconds > 0 || builder.len() == 0 {
            builder.append(format!("{}秒", seconds));
        }
        builder.string().unwrap()
    }
}

pub trait StringBuilderExt {
    fn append_with_line<T: string_builder::ToBytes>(&mut self, buf: T) -> &mut Self;
    fn append_line(&mut self) -> &mut Self;
}

impl StringBuilderExt for string_builder::Builder {
    fn append_with_line<T: string_builder::ToBytes>(&mut self, buf: T) -> &mut Self {
        self.append(buf);
        self.append("\n");
        self
    }
    fn append_line(&mut self) -> &mut Self {
        self.append("\n");
        self
    }
}

pub trait ItemExt {
    fn pub_date_utc(&self) -> DateTime<Utc>;
}

impl ItemExt for rss::Item {
    fn pub_date_utc(&self) -> DateTime<Utc> {
        let pub_date_str = self.pub_date().unwrap();
        DateTime::parse_from_rfc2822(pub_date_str)
            .unwrap()
            .with_timezone(&Utc)
    }
}