use crate::services::steam_api::SteamService;
use futures::stream::{self, StreamExt};
use sqlx::{PgPool, Row};
use std::{env, sync::Arc, time::Duration};

pub async fn start_verification_worker(pool: PgPool, steam_service: Arc<SteamService>) {
    tracing::info!("Verification worker started");

    loop {
        let manual_rows = fetch_pending_rows(&pool, "player_verifications").await;
        let cache_rows = fetch_pending_rows(&pool, "player_cache").await;
        let had_work = !manual_rows.is_empty() || !cache_rows.is_empty();

        if !manual_rows.is_empty() {
            process_batch(&pool, &steam_service, manual_rows, "player_verifications").await;
        }

        if !cache_rows.is_empty() {
            process_batch(&pool, &steam_service, cache_rows, "player_cache").await;
        }

        tokio::time::sleep(worker_sleep_duration(had_work)).await;
    }
}

async fn fetch_pending_rows(pool: &PgPool, table: &str) -> Vec<sqlx::postgres::PgRow> {
    let query = format!(
        "SELECT steam_id
         FROM {}
         WHERE status = 'pending'
         ORDER BY created_at ASC
         LIMIT {}",
        table,
        batch_size()
    );

    match sqlx::query(&query).fetch_all(pool).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(
                "Failed to fetch pending verification rows from {}: {}",
                table,
                e
            );
            Vec::new()
        }
    }
}

async fn process_batch(
    pool: &PgPool,
    steam_service: &Arc<SteamService>,
    rows: Vec<sqlx::postgres::PgRow>,
    table: &str,
) {
    stream::iter(rows)
        .for_each_concurrent(worker_concurrency(), |row| {
            let pool = pool.clone();
            let steam_service = steam_service.clone();
            let table = table.to_string();
            async move {
                let steam_id: String = row.get("steam_id");
                if let Err(e) = fetch_and_save_data(&pool, &steam_service, &steam_id, &table).await
                {
                    tracing::error!("Verification error for {} in {}: {:?}", steam_id, table, e);
                }
            }
        })
        .await;
}

async fn fetch_and_save_data(
    pool: &PgPool,
    steam_service: &SteamService,
    steam_id: &str,
    table: &str,
) -> anyhow::Result<()> {
    if steam_id.eq_ignore_ascii_case("BOT") {
        update_data(pool, table, steam_id, Some(0), Some(0), Some(0.0)).await?;
        return Ok(());
    }

    let resolved_id = steam_service
        .resolve_steam_id(steam_id)
        .await
        .unwrap_or_else(|| steam_id.to_string());

    let (gokz_rating, level, playtime) = tokio::join!(
        steam_service.get_gokz_rating(&resolved_id),
        steam_service.get_steam_level(&resolved_id),
        steam_service.get_csgo_playtime_minutes(&resolved_id)
    );

    update_data(
        pool,
        table,
        steam_id,
        Some(level.unwrap_or(0)),
        Some(playtime.unwrap_or(0)),
        Some(gokz_rating.unwrap_or(0.0)),
    )
    .await?;

    Ok(())
}

async fn update_data(
    pool: &PgPool,
    table: &str,
    steam_id: &str,
    level: Option<i32>,
    playtime: Option<i32>,
    gokz_rating: Option<f64>,
) -> anyhow::Result<()> {
    let query = format!(
        "UPDATE {}
         SET status = 'verified', steam_level = $1, playtime_minutes = $2, gokz_rating = $3, updated_at = NOW()
         WHERE steam_id = $4",
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

fn batch_size() -> usize {
    env::var("VERIFICATION_BATCH_SIZE")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20)
}

fn worker_concurrency() -> usize {
    env::var("VERIFICATION_FETCH_CONCURRENCY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(10)
}

fn worker_sleep_duration(had_work: bool) -> Duration {
    let env_key = if had_work {
        "VERIFICATION_WORKER_ACTIVE_SLEEP_MS"
    } else {
        "VERIFICATION_WORKER_IDLE_SLEEP_MS"
    };

    let default_ms = if had_work { 750 } else { 3000 };

    Duration::from_millis(
        env::var(env_key)
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(default_ms),
    )
}
