use std::collections::HashSet;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use encoding_rs::*;
use once_cell::sync::Lazy;
use regex::Regex;
use sxd_xpath::{evaluate_xpath, Value::Nodeset};

use crate::TagConfig;

static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^\w]+").unwrap()); // 単語文字以外の文字にマッチする正規表現
static COMBINE_TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"#\s(\w)").unwrap()); // #と単語文字の間にスペースがある場合にマッチする正規表現

pub trait ISO8601 {
    fn to_iso8601(&self) -> String;
}

impl ISO8601 for Duration {
    fn to_iso8601(&self) -> String {
        let total_seconds = self.num_seconds();
        let years = total_seconds / 31_536_000;
        let months = (total_seconds % 31_536_000) / 2_592_000;
        let days = (total_seconds % 2_592_000) / 86_400;
        let hours = (total_seconds % 86_400) / 36_00;
        let minutes = (total_seconds % 36_00) / 60;
        let seconds = total_seconds % 60;
        let mut builder = string_builder::Builder::default();
        builder.append("P");
        if years > 0 {
            builder.append(format!("{}Y", years));
        }
        if months > 0 {
            builder.append(format!("{}M", months));
        }
        if days > 0 {
            builder.append(format!("{}D", days));
        }
        builder.append("T");
        if hours > 0 {
            builder.append(format!("{}H", hours));
        }
        if minutes > 0 {
            builder.append(format!("{}M", minutes));
        }
        if seconds > 0 || builder.len() == 0 {
            builder.append(format!("{}S", seconds));
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
    fn pub_date_utc(&self) -> Option<&DateTime<Utc>>;
    fn pub_date_utc_or<'a>(&'a self, or: &'a DateTime<Utc>) -> &'a DateTime<Utc>;
    async fn to_status(&self, id: String, config: &TagConfig) -> anyhow::Result<String>;
}

#[async_trait]
impl ItemExt for feed_rs::model::Entry {
    fn pub_date_utc(&self) -> Option<&DateTime<Utc>> {
        if let Some(p) = &self.published {
            Some(&p)
        } else if let Some(u) = &self.updated {
            Some(&u)
        } else {
            None
        }
    }

    fn pub_date_utc_or<'a>(&'a self, or: &'a DateTime<Utc>) -> &'a DateTime<Utc> {
        if let Some(p) = self.pub_date_utc() {
            &p
        } else {
            or
        }
    }

    async fn to_status(&self, id: String, config: &TagConfig) -> anyhow::Result<String> {
        let mut b = string_builder::Builder::default();
        let mut title = None;
        if let Some(t) = &self.title {
            let head = COMBINE_TAG_RE
                .replace_all(t.content.as_str(), "#$1")
                .to_string();
            b.append_with_line(head);
            title = Some(&t.content);
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
        tags.extend(config.always.clone());

        for link in &self.links {
            let response = reqwest::get(&link.href).await?;
            let contents = decode_text(response).await?;
            let package = sxd_html::parse_html(&contents);
            let doc = package.as_document();

            // keywordsがfalseに設定されている場合はメタキーワードを抽出しない
            if config.keywords.unwrap_or(true) {
                if let Nodeset(nodes) = evaluate_xpath(&doc, "//meta[@name='keywords']/@content")? {
                    for node in nodes {
                        for keyword in node.string_value().split(',') {
                            tags.push(keyword.trim().to_string());
                        }
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

        // 大文字小文字を区別しない重複排除
        let mut seen = HashSet::new();
        tags.retain(|e| seen.insert(e.to_uppercase()));
        if let Some(title) = title {
            tags.retain(|e| !e.contains(title));
        }
        tags.retain(|e| !e.is_empty() && !e.chars().all(char::is_numeric));
        if !tags.is_empty() {
            // 空行を入れるとMastodonで見やすくなる
            b.append_line();
            b.append(
                tags.iter()
                    .map(|t| {
                        format!(
                            "#{}",
                            TAG_RE.replace_all(&t, "_").trim_matches(|c| c == '_')
                        )
                    })
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
    let Ok(Nodeset(nodes)) = evaluate_xpath(&doc, "//meta[@http-equiv='content-type']/@content")
    else {
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
