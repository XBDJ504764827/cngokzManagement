use sqlx::mysql::MySqlPoolOptions;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = "mysql://data:datadata@192.168.0.130/zzzXBDJBans"; // Hardcoded from .env
    
    let pool = MySqlPoolOptions::new()
        .max_connections(5)
        .connect(database_url).await?;

    let in_whitelist: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM whitelist WHERE steam_id = '76561198298405388'"
    )
    .fetch_one(&pool)
    .await?;

    println!("DEBUG RESULT: In Whitelist = {}", in_whitelist);
    
    Ok(())
}
