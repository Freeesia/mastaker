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
    let mut opt = ConnectOptions::new(DATABASE_URL.clone());
    opt.connect_timeout(std::time::Duration::from_secs(25))
        .acquire_timeout(std::time::Duration::from_secs(60));
    loop {
        match Database::connect(opt.clone()).await {
            Ok(db) => {
                break Ok(db);
            }
            Err(err) => {
                println!("Failed to connect to database: {}, Retrying in 5 seconds", err);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
    }
}

pub async fn setup_tables() -> Result<(), DbErr> {
    let db = setup_connection().await?;
    let backend = db.get_database_backend();
    let schema = Schema::new(backend);
    let schema_manager = SchemaManager::new(&db);
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
