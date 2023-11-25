use crate::constants::*;
use crate::schema::*;
use crate::{feed_info, post_item};
use std::{env, fs::File};

use feed_info::Entity as FeedInfo;
use post_item::Entity as PostItem;
use sea_orm::*;
use sea_orm_migration::SchemaManager;

pub fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    let path =
        env::var(FEED_CONFIG_PATH_ENV).expect(&format!("{} must be set", FEED_CONFIG_PATH_ENV));
    let file = File::open(path)?;
    let config: Config = serde_yaml::from_reader(file)?;
    Ok(config)
}

pub async fn setup_connection() -> Result<DatabaseConnection, DbErr> {
    let database_url =
        env::var(DATABASE_URL_ENV).expect(&format!("{} must be set", DATABASE_URL_ENV));
    Database::connect(database_url).await
}

pub async fn setup_tables(db: &DatabaseConnection) -> Result<(), DbErr> {
    let backend = db.get_database_backend();
    let schema = Schema::new(backend);
    let schema_manager = SchemaManager::new(db);
    schema_manager
        .create_table(
            schema
                .create_table_from_entity(PostItem)
                .if_not_exists()
                .take(),
        )
        .await?;
    schema_manager
        .create_table(
            schema
                .create_table_from_entity(FeedInfo)
                .if_not_exists()
                .take(),
        )
        .await?;
    for mut stmt in schema.create_index_from_entity(PostItem) {
        schema_manager
            .create_index(stmt.if_not_exists().take())
            .await?;
    }
    for mut stmt in schema.create_index_from_entity(FeedInfo) {
        schema_manager
            .create_index(stmt.if_not_exists().take())
            .await?;
    }
    Ok(())
}
