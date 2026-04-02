use sqlx::postgres::{PgPool, PgPoolOptions};
use std::env;

pub async fn establish_connection() -> PgPool {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    if should_auto_create_database() {
        use sqlx::migrate::MigrateDatabase;

        if sqlx::Postgres::database_exists(&database_url)
            .await
            .unwrap_or(false)
        {
            println!("Database already exists.");
        } else {
            println!("Database does not exist, creating...");
            match sqlx::Postgres::create_database(&database_url).await {
                Ok(_) => println!("Database created successfully."),
                Err(e) => {
                    println!("Failed to create database: {}", e);
                    panic!("Could not create database. Error: {}", e);
                }
            }
        }
    }

    PgPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await
        .expect("Failed to create pool")
}

fn should_auto_create_database() -> bool {
    env::var("AUTO_CREATE_DATABASE")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}
