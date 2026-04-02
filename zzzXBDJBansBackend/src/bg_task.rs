use crate::utils::rcon::send_command;
use crate::AppState;
use chrono::{DateTime, Utc};
use futures::stream::{self, StreamExt};
use sqlx::FromRow;
use std::{
    collections::{HashMap, HashSet},
    env,
    sync::Arc,
};
use tokio::{
    sync::Mutex,
    time::{interval, timeout, Duration},
};

#[derive(Clone, FromRow)]
struct BackgroundServer {
    id: i64,
    ip: String,
    port: i32,
    rcon_password: Option<String>,
}

#[derive(Clone, FromRow)]
struct ActiveIpBan {
    ip: String,
    duration: String,
    expires_at: Option<DateTime<Utc>>,
}

struct ConnectedPlayer {
    userid: String,
    name: String,
    steam_id: String,
    ip: String,
}

pub async fn start_background_task(state: Arc<AppState>) {
    tracing::info!("Background task started: ban cleanup and player IP enforcement");
    let mut ticker = interval(Duration::from_secs(background_interval_seconds()));

    loop {
        ticker.tick().await;
        if let Err(e) = run_background_cycle(&state).await {
            tracing::error!("Background task error: {}", e);
        }
    }
}

async fn run_background_cycle(
    state: &Arc<AppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    cleanup_expired_bans(state).await?;
    enforce_ip_bans(state).await?;
    Ok(())
}

async fn cleanup_expired_bans(state: &Arc<AppState>) -> Result<(), sqlx::Error> {
    let done = sqlx::query(
        "UPDATE bans
         SET status = 'expired'
         WHERE status = 'active'
           AND expires_at IS NOT NULL
           AND expires_at < NOW()",
    )
    .execute(&state.db)
    .await?;

    if done.rows_affected() > 0 {
        tracing::info!(
            "Marked {} expired bans during background cleanup",
            done.rows_affected()
        );
    }

    Ok(())
}

async fn enforce_ip_bans(
    state: &Arc<AppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ip_bans = sqlx::query_as::<_, ActiveIpBan>(
        "SELECT ip, duration, expires_at
         FROM bans
         WHERE status = 'active'
           AND ban_type = 'ip'
           AND ip <> ''
           AND (expires_at IS NULL OR expires_at > NOW())",
    )
    .fetch_all(&state.db)
    .await?;

    if ip_bans.is_empty() {
        return Ok(());
    }

    let active_steamids = sqlx::query_scalar::<_, String>(
        "SELECT steam_id
         FROM bans
         WHERE status = 'active'
           AND steam_id IS NOT NULL
           AND steam_id <> ''
           AND (expires_at IS NULL OR expires_at > NOW())",
    )
    .fetch_all(&state.db)
    .await?;

    let servers = sqlx::query_as::<_, BackgroundServer>(
        "SELECT id, ip, port, rcon_password
         FROM servers
         ORDER BY id ASC",
    )
    .fetch_all(&state.db)
    .await?;

    let ip_ban_map = Arc::new(
        ip_bans
            .into_iter()
            .map(|ban| (ban.ip.clone(), ban))
            .collect::<HashMap<_, _>>(),
    );
    let active_steamids = Arc::new(Mutex::new(
        active_steamids.into_iter().collect::<HashSet<_>>(),
    ));
    let state = Arc::clone(state);

    stream::iter(servers)
        .for_each_concurrent(server_check_concurrency(), |server| {
            let state = Arc::clone(&state);
            let ip_ban_map = Arc::clone(&ip_ban_map);
            let active_steamids = Arc::clone(&active_steamids);
            async move {
                if let Err(e) = check_server(&state, server, ip_ban_map, active_steamids).await {
                    tracing::warn!("Server background check failed: {}", e);
                }
            }
        })
        .await;

    Ok(())
}

async fn check_server(
    state: &Arc<AppState>,
    server: BackgroundServer,
    ip_ban_map: Arc<HashMap<String, ActiveIpBan>>,
    active_steamids: Arc<Mutex<HashSet<String>>>,
) -> Result<(), String> {
    let password = match server.rcon_password.as_deref() {
        Some(password) if !password.is_empty() => password,
        _ => {
            maybe_update_server_status(state, &server, "unknown").await;
            return Ok(());
        }
    };

    let address = format!("{}:{}", server.ip, server.port);
    let output = execute_rcon(&address, password, "status").await;

    let output = match output {
        Ok(output) => {
            maybe_update_server_status(state, &server, "online").await;
            output
        }
        Err(e) => {
            maybe_update_server_status(state, &server, "offline").await;
            return Err(e);
        }
    };

    for player in parse_status_output(&output) {
        let Some(ip_ban) = ip_ban_map.get(&player.ip) else {
            continue;
        };

        let known_active_ban = {
            let active = active_steamids.lock().await;
            active.contains(&player.steam_id)
        };

        if known_active_ban {
            let _ = execute_rcon(
                &address,
                password,
                &format!("kickid {} \"Banned IP Detected\"", player.userid),
            )
            .await;
            continue;
        }

        tracing::info!(
            "Background task caught user bypassing IP ban. Server={}, IP={}, SteamID={}, Name={}",
            server.id,
            player.ip,
            player.steam_id,
            player.name
        );

        let inserted = insert_background_ban(state, &player, ip_ban, server.id)
            .await
            .map_err(|e| e.to_string())?;

        if inserted {
            {
                let mut active = active_steamids.lock().await;
                active.insert(player.steam_id.clone());
            }

            let _ = execute_rcon(
                &address,
                password,
                &format!(
                    "sm_ban #{} {} \"{}\"",
                    player.userid, ip_ban.duration, "同IP关联封禁 (Detected online with Banned IP)"
                ),
            )
            .await;
        } else {
            {
                let mut active = active_steamids.lock().await;
                active.insert(player.steam_id.clone());
            }

            let _ = execute_rcon(
                &address,
                password,
                &format!("kickid {} \"Banned IP Detected\"", player.userid),
            )
            .await;
        }
    }

    Ok(())
}

async fn insert_background_ban(
    state: &Arc<AppState>,
    player: &ConnectedPlayer,
    ip_ban: &ActiveIpBan,
    server_id: i64,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO bans (
            name, steam_id, ip, ban_type, reason, duration, admin_name, expires_at, created_at, status, server_id
         )
         SELECT
            $1, $2, $3, 'account', $4, $5, 'System (BG Monitor)', $6, NOW(), 'active', $7
         WHERE NOT EXISTS (
            SELECT 1
            FROM bans
            WHERE status = 'active'
              AND steam_id = $2
              AND (expires_at IS NULL OR expires_at > NOW())
         )",
    )
    .bind(&player.name)
    .bind(&player.steam_id)
    .bind(&player.ip)
    .bind("同IP关联封禁 (Detected online with Banned IP)")
    .bind(&ip_ban.duration)
    .bind(ip_ban.expires_at)
    .bind(server_id)
    .execute(&state.db)
    .await?;

    Ok(result.rows_affected() > 0)
}

fn parse_status_output(output: &str) -> Vec<ConnectedPlayer> {
    let mut players = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if !line.starts_with('#') {
            continue;
        }

        let Some(first_quote) = line.find('"') else {
            continue;
        };
        let Some(last_quote) = line.rfind('"') else {
            continue;
        };

        if first_quote >= last_quote {
            continue;
        }

        let pre_name = line[..first_quote].trim();
        let pre_parts: Vec<&str> = pre_name.split_whitespace().collect();
        let userid = pre_parts.last().copied().unwrap_or_default();
        if userid.is_empty() || userid == "#" {
            continue;
        }

        let player_name = line[first_quote + 1..last_quote].to_string();
        let after_name = line[last_quote + 1..].trim();
        let fields: Vec<&str> = after_name.split_whitespace().collect();
        if fields.len() < 2 {
            continue;
        }

        let steam_id = fields[0];
        let ip_only = fields
            .last()
            .and_then(|value| value.split(':').next())
            .unwrap_or_default();

        if steam_id == "BOT" || ip_only.is_empty() {
            continue;
        }

        players.push(ConnectedPlayer {
            userid: userid.to_string(),
            name: player_name,
            steam_id: steam_id.to_string(),
            ip: ip_only.to_string(),
        });
    }

    players
}

async fn execute_rcon(address: &str, password: &str, command: &str) -> Result<String, String> {
    let timeout_secs = rcon_timeout_seconds();
    match timeout(
        Duration::from_secs(timeout_secs),
        send_command(address, password, command),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(format!("RCON command timed out after {}s", timeout_secs)),
    }
}

async fn maybe_update_server_status(
    state: &Arc<AppState>,
    server: &BackgroundServer,
    next_status: &str,
) {
    update_cached_server_status(state, server.id, next_status).await;
}

async fn update_cached_server_status(state: &Arc<AppState>, server_id: i64, status: &str) {
    if let Err(e) = sqlx::query(
        "UPDATE servers SET cached_status = $1, status_checked_at = NOW() WHERE id = $2",
    )
    .bind(status)
    .bind(server_id)
    .execute(&state.db)
    .await
    {
        tracing::warn!(
            "Failed to update cached status for server {} to {}: {}",
            server_id,
            status,
            e
        );
    }
}

fn background_interval_seconds() -> u64 {
    env::var("BG_TASK_INTERVAL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(60)
}

fn server_check_concurrency() -> usize {
    env::var("BG_SERVER_CHECK_CONCURRENCY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(4)
}

fn rcon_timeout_seconds() -> u64 {
    env::var("BG_RCON_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(8)
}
