use chrono::{DateTime, Duration, Utc};
use once_cell::sync::Lazy;
use regex::Regex;

static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^\w]").unwrap()); // 単語文字以外の文字にマッチする正規表現

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
    fn record_id(&self) -> String;
    fn pub_date_utc(&self) -> Option<DateTime<Utc>>;
    fn pub_date_utc_or(&self, or: DateTime<Utc>) -> DateTime<Utc>;
    fn to_status(&self) -> String;
}

impl ItemExt for feed_rs::model::Entry {
    fn record_id(&self) -> String {
        self.id.clone()
    }

    fn pub_date_utc(&self) -> Option<DateTime<Utc>> {
        if let Some(p) = self.published {
            Some(p)
        } else if let Some(u) = self.updated {
            Some(u)
        } else {
            None
        }
    }

    fn pub_date_utc_or(&self, or: DateTime<Utc>) -> DateTime<Utc> {
        if let Some(p) = self.pub_date_utc() {
            p
        } else {
            or
        }
    }

    fn to_status(&self) -> String {
        let mut b = string_builder::Builder::default();
        if let Some(t) = &self.title {
            b.append_with_line(t.content.as_str());
        }
        for link in &self.links {
            b.append_with_line(link.href.as_str());
        }
        let tags = self
            .categories
            .iter()
            .map(|c| {
                format!(
                    "#{}",
                    TAG_RE.replace_all(&c.label.as_ref().unwrap_or(&c.term), "_")
                )
            })
            .collect::<Vec<String>>();
        if tags.len() > 0 {
            // 空行を入れるとMastodonで見やすくなる
            b.append_line();
            b.append(tags.join(" "));
        }
        b.string().unwrap()
    }
}
