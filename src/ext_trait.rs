use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use once_cell::sync::Lazy;
use regex::Regex;
use sxd_xpath::{evaluate_xpath, Value::Nodeset};
use encoding_rs::*;

use crate::TagConfig;

static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^\w]+").unwrap()); // 単語文字以外の文字にマッチする正規表現

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

#[async_trait]
pub trait ItemExt {
    fn pub_date_utc(&self) -> Option<DateTime<Utc>>;
    fn pub_date_utc_or(&self, or: DateTime<Utc>) -> DateTime<Utc>;
    async fn to_status(
        &self,
        id: String,
        config: &Option<TagConfig>,
    ) -> Result<String, Box<dyn std::error::Error>>;
}

#[async_trait]
impl ItemExt for feed_rs::model::Entry {
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

    async fn to_status(
        &self,
        id: String,
        config: &Option<TagConfig>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut b = string_builder::Builder::default();
        if let Some(t) = &self.title {
            b.append_with_line(t.content.as_str());
        }
        for link in &self.links {
            b.append_with_line(link.href.as_str());
        }
        // すごいメモリ無駄にしている気がする…
        let mut tags = std::iter::once(id)
            .chain(
                self.categories
                    .iter()
                    .map(|c| c.label.clone().unwrap_or(c.term.clone())),
            )
            .collect::<Vec<String>>();
        if let Some(config) = config {
            tags.extend(config.always.clone());

            for link in &self.links {
                let response = reqwest::get(&link.href).await?;
                let contents = decode_text(response).await?;
                let package = sxd_html::parse_html(&contents);
                let doc = package.as_document();
                if let Nodeset(nodes) = evaluate_xpath(&doc, "//meta[@name='keywords']/@content")? {
                    for node in nodes {
                        for keyword in node.string_value().split(',') {
                            tags.push(keyword.trim().to_string());
                        }
                    }
                }
                // xpathがない場合は無視
                let Some(xpath) = &config.xpath else { continue };
                let Ok(Nodeset(nodes)) = evaluate_xpath(&doc, xpath) else {
                    // TODO: Sentryに送る
                    continue;
                };
                for node in nodes {
                    tags.push(node.string_value().trim().to_string());
                }
            }

            if !config.replace.is_empty() {
                let replace = config
                    .replace
                    .iter()
                    .filter_map(|i| Regex::new(i).ok())
                    .collect::<Vec<Regex>>();
                tags = tags
                    .into_iter()
                    .map(|t| {
                        replace
                            .iter()
                            .fold(t, |t, r| r.replace_all(&t, "").to_string())
                    })
                    .collect::<Vec<String>>();
            }

            if !config.ignore.is_empty() {
                let ignore = config
                    .ignore
                    .iter()
                    .filter_map(|i| Regex::new(i).ok())
                    .collect::<Vec<Regex>>();
                tags = tags
                    .into_iter()
                    .filter(|t| !ignore.iter().any(|r| r.is_match(t)))
                    .collect::<Vec<String>>();
            }
        }
        if !tags.is_empty() {
            // 空行を入れるとMastodonで見やすくなる
            b.append_line();
            b.append(
                tags.iter()
                    .map(|t| format!("#{}", TAG_RE.replace_all(&t, "_").trim_matches(|c| c == '_')))
                    .collect::<Vec<String>>()
                    .join(" "),
            );
        }
        Ok(b.string().unwrap())
    }
}

async fn decode_text(res: reqwest::Response) -> Result<String, reqwest::Error> {
    let encoding = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<mime::Mime>().ok())
        .and_then(|m| {
            m.get_param(mime::CHARSET)
                .map(|charset| charset.to_string())
        })
        .and_then(|e| Encoding::for_label(e.as_bytes()));

    let full = res.bytes().await?;

    // ヘッダーにcharsetがある場合はそれを優先する
    if let Some(en) = encoding {
        let (text, _, _) = en.decode(&full);
        return Ok(text.into_owned());
    }

    // HTMLのmetaタグにcharsetがある場合はそれを使う
    let (tmp, _, _) = UTF_8.decode(&full);
    let package = sxd_html::parse_html(&tmp);
    let doc = package.as_document();
    let Ok(Nodeset(nodes)) =
        evaluate_xpath(&doc, "//meta[@http-equiv='content-type']/@content") else {
        return Ok(tmp.into_owned());
    };
    let encoding = nodes
        .document_order_first()
        .and_then(|first| first.string_value().parse::<mime::Mime>().ok())
        .and_then(|mime| {
            mime.get_param(mime::CHARSET)
                .map(|charset| charset.to_string())
        })
        .and_then(|e| Encoding::for_label(e.as_bytes()));
    if let Some(encoding) = encoding {
        let (text, _, _) = encoding.decode(&full);
        Ok(text.into_owned())
    } else {
        Ok(tmp.into_owned())
    }
}
