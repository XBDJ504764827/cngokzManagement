use std::time::Duration;
use sqlx::{MySqlPool, Row};
use crate::services::steam_api::SteamService;

use futures::stream::{self, StreamExt};
use std::sync::Arc;

pub async fn start_verification_worker(pool: MySqlPool) {
    let steam_service = Arc::new(SteamService::new());
    tracing::info!("Verification Worker started.");

    loop {
        // 1. Priority: Manual Verifications
        if let Ok(rows) = sqlx::query("SELECT steam_id FROM player_verifications WHERE status = 'pending' LIMIT 20")
            .fetch_all(&pool)
            .await
        {
            process_batch(&pool, &steam_service, rows, "player_verifications").await;
        }

        // 2. Secondary: Player Cache
        if let Ok(rows) = sqlx::query("SELECT steam_id FROM player_cache WHERE status = 'pending' LIMIT 20")
            .fetch_all(&pool)
            .await
        {
            process_batch(&pool, &steam_service, rows, "player_cache").await;
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn process_batch(pool: &MySqlPool, steam_service: &Arc<SteamService>, rows: Vec<sqlx::mysql::MySqlRow>, table: &str) {
    stream::iter(rows)
        .for_each_concurrent(10, |row| {
            let pool = pool.clone();
            let steam_service = steam_service.clone();
            let table = table.to_string(); // String is cheap enough for 20 items
            async move {
                let steam_id: String = row.get("steam_id");
                if let Err(e) = fetch_and_save_data(&pool, &steam_service, &steam_id, &table).await {
                     tracing::error!("Verification error for {}: {:?}", steam_id, e);
                }
            }
        })
        .await;
}

/// 从 API 获取数据并保存到数据库
async fn fetch_and_save_data(
    pool: &MySqlPool, 
    steam_service: &SteamService, 
    steam_id: &str, 
    table: &str
) -> anyhow::Result<()> {
    if steam_id.eq_ignore_ascii_case("BOT") {
        update_data(pool, table, steam_id, Some(0), Some(0), Some(0.0)).await?;
        return Ok(());
    }

    let resolved_id = steam_service.resolve_steam_id(steam_id).await
        .unwrap_or_else(|| steam_id.to_string());

    let gokz_rating = steam_service.get_gokz_rating(&resolved_id).await.unwrap_or(0.0);
    let level = steam_service.get_steam_level(&resolved_id).await.unwrap_or(0);
    let playtime = steam_service.get_csgo_playtime_minutes(&resolved_id).await.unwrap_or(0);

    update_data(pool, table, steam_id, Some(level), Some(playtime), Some(gokz_rating)).await?;
    Ok(())
}

/// 更新数据并将状态设为 verified
async fn update_data(
    pool: &MySqlPool, 
    table: &str, 
    steam_id: &str, 
    level: Option<i32>, 
    playtime: Option<i32>, 
    gokz_rating: Option<f64>
) -> anyhow::Result<()> {
    let query = format!(
        "UPDATE {} SET status = 'verified', steam_level = ?, playtime_minutes = ?, gokz_rating = ?, updated_at = NOW() WHERE steam_id = ?", 
        table
    );
    sqlx::query(&query)
        .bind(level)
        .bind(playtime)
        .bind(gokz_rating)
        .bind(steam_id)
        .execute(pool)
        .await?;
    Ok(())
}
