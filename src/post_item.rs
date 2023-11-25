use chrono::Utc;
use feed_rs::model::Entry;
use sea_orm::{entity::prelude::*, Set};

use crate::ext_trait::ItemExt;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "post_item")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(indexed)]
    pub source: String,
    pub title: String,
    pub link: String,
    #[sea_orm(indexed)]
    pub post_id: Option<String>,
    #[sea_orm(indexed)]
    pub pub_date: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl Entity {
    pub async fn insert(
        db: &DatabaseConnection,
        source: &String,
        entry: &Entry,
    ) -> Result<Model, anyhow::Error> {
        let post = ActiveModel {
            source: Set(source.to_owned()),
            title: Set(entry.title.as_ref().unwrap().content.to_owned()),
            link: Set(entry.links.get(0).unwrap().href.clone()),
            pub_date: Set(*entry.pub_date_utc_or(&Utc::now())),
            ..Default::default()
        }
        .insert(db)
        .await?;
        Ok(post)
    }
}
