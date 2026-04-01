use sqlx::mysql::MySqlPoolOptions;
use std::env;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    
    // Connect to base url (hacky parse)
    let base_url = if let Some(idx) = database_url.rfind('/') {
        &database_url[..idx+1]
    } else {
        &database_url
    };
    
    println!("Connecting to {} to drop database...", base_url);

    let pool = MySqlPoolOptions::new()
        .connect(base_url)
        .await
        .expect("Failed to connect to server");

    sqlx::query("DROP DATABASE IF EXISTS zzzXBDJBans")
        .execute(&pool)
        .await
        .expect("Failed to drop database");
        
    println!("Database dropped successfully.");
}
