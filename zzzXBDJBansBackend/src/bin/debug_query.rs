use sqlx::postgres::PgPoolOptions;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url).await?;

    let in_whitelist: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM whitelist WHERE steam_id = '76561198298405388'"
    )
    .fetch_one(&pool)
    .await?;

    println!("DEBUG RESULT: In Whitelist = {}", in_whitelist);
    
    Ok(())
}
