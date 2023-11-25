use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub base_url: String,
    pub feeds: Vec<FeedConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeedConfig {
    pub id: String,
    pub url: String,
    pub token: String,
    pub tag: Option<TagConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TagConfig {
    pub always: Vec<String>,
    pub ignore: Vec<String>,
    pub replace: Vec<String>,
    pub xpath: Option<String>,
}
