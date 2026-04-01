use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use std::env;

pub async fn establish_connection() -> MySqlPool {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    
    // Parse URL to find the database name part
    // Format: mysql://user:pass@host:port/dbname
    let _db_name = database_url.split('/').last().expect("Invalid DB URL format");
    let _server_url = database_url.get(..database_url.rfind('/').unwrap_or(database_url.len())).unwrap_or(&database_url);

    use sqlx::migrate::MigrateDatabase;

    if sqlx::MySql::database_exists(&database_url).await.unwrap_or(false) {
        println!("Database already exists.");
    } else {
        println!("Database does not exist, creating...");
        match sqlx::MySql::create_database(&database_url).await {
            Ok(_) => println!("Database created successfully."),
            Err(e) => {
                println!("Failed to create database: {}", e);
                // If we fail here, we likely crash next. 
                // We'll panic with a clear message.
                panic!("Could not create database. Check permissions for user 'data'. Error: {}", e);
            }
        }
    }

    MySqlPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await
        .expect("Failed to create pool")
}
