use crate::handlers::auth::Claims;
use crate::models::server::{
    CheckServerRequest, CreateGroupRequest, CreateServerRequest, GroupWithServers, Server,
    ServerGroup, ServerStatusSummary, ServerSummary, UpdateServerRequest,
};
use crate::utils::log_admin_action; // Ensure this is accessible
use crate::utils::rcon::check_rcon;
use crate::AppState;
use axum::{
    extract::{Extension, Json, Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;

// --- Groups ---

#[utoipa::path(
    get,
    path = "/api/server-groups",
    responses(
        (status = 200, description = "List server groups with servers", body = Vec<GroupWithServers>)
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn list_server_groups(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let groups =
        match sqlx::query_as::<_, ServerGroup>("SELECT * FROM server_groups ORDER BY id ASC")
            .fetch_all(&state.db)
            .await
        {
            Ok(groups) => groups,
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        };

    let servers = match sqlx::query_as::<_, Server>("SELECT * FROM servers ORDER BY id ASC")
        .fetch_all(&state.db)
        .await
    {
        Ok(servers) => servers,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    // Combine
    let mut result = Vec::new();
    for g in groups {
        let group_servers: Vec<ServerSummary> = servers
            .iter()
            .filter(|s| s.group_id == g.id)
            .map(|s| ServerSummary {
                id: s.id,
                group_id: s.group_id,
                name: s.name.clone(),
                ip: s.ip.clone(),
                port: s.port,
                created_at: s.created_at,
                verification_enabled: s.verification_enabled,
                status: s.cached_status.clone(),
                status_checked_at: s.status_checked_at,
            })
            .collect();

        result.push(GroupWithServers {
            id: g.id,
            name: g.name,
            servers: group_servers,
        });
    }

    (StatusCode::OK, Json(result)).into_response()
}

#[utoipa::path(
    get,
    path = "/api/server-statuses",
    responses(
        (status = 200, description = "List server online statuses", body = Vec<ServerStatusSummary>)
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn list_server_statuses(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let statuses = match sqlx::query_as::<_, ServerStatusSummary>(
        "SELECT id AS server_id, cached_status AS status, status_checked_at FROM servers ORDER BY id ASC"
    )
        .fetch_all(&state.db)
        .await
    {
        Ok(statuses) => statuses,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    (StatusCode::OK, Json(statuses)).into_response()
}

#[utoipa::path(
    post,
    path = "/api/server-groups",
    request_body = CreateGroupRequest,
    responses(
        (status = 201, description = "Group created"),
        (status = 500, description = "Server Error")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn create_group(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
    Json(payload): Json<CreateGroupRequest>,
) -> impl IntoResponse {
    let result = sqlx::query("INSERT INTO server_groups (name) VALUES ($1)")
        .bind(&payload.name)
        .execute(&state.db)
        .await;

    match result {
        Ok(_) => {
            let _ = log_admin_action(
                &state.db,
                &user.sub,
                "create_group",
                &payload.name,
                "Created server group",
            )
            .await;
            (StatusCode::CREATED, Json("Group created")).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/server-groups/{id}",
    params(
        ("id" = i64, Path, description = "Group ID")
    ),
    responses(
        (status = 200, description = "Group deleted")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn delete_group(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let result = sqlx::query("DELETE FROM server_groups WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await;

    match result {
        Ok(_) => {
            let _ = log_admin_action(
                &state.db,
                &user.sub,
                "delete_group",
                &format!("ID: {}", id),
                "Deleted server group",
            )
            .await;
            (StatusCode::OK, Json("Group deleted")).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// --- Servers ---

#[utoipa::path(
    post,
    path = "/api/servers",
    request_body = CreateServerRequest,
    responses(
        (status = 201, description = "Server created")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn create_server(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
    Json(payload): Json<CreateServerRequest>,
) -> impl IntoResponse {
    let result = sqlx::query(
        "INSERT INTO servers (group_id, name, ip, port, rcon_password, verification_enabled) VALUES ($1, $2, $3, $4, $5, $6)"
    )
    .bind(payload.group_id)
    .bind(&payload.name)
    .bind(&payload.ip)
    .bind(payload.port)
    .bind(&payload.rcon_password)
    .bind(payload.verification_enabled.unwrap_or(true))
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => {
            let _ = log_admin_action(
                &state.db,
                &user.sub,
                "create_server",
                &payload.name,
                &format!("{}:{}", payload.ip, payload.port),
            )
            .await;
            (StatusCode::CREATED, Json("Server created")).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    put,
    path = "/api/servers/{id}",
    params(
        ("id" = i64, Path, description = "Server ID")
    ),
    request_body = UpdateServerRequest,
    responses(
        (status = 200, description = "Server updated")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn update_server(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateServerRequest>,
) -> impl IntoResponse {
    let mut tx = match state.db.begin().await {
        Ok(tx) => tx,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let existing =
        match sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
        {
            Ok(Some(server)) => server,
            Ok(None) => return (StatusCode::NOT_FOUND, Json("Server not found")).into_response(),
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        };

    let name = payload.name.unwrap_or(existing.name);
    let ip = payload.ip.unwrap_or(existing.ip);
    let port = payload.port.unwrap_or(existing.port);
    let rcon_password = payload.rcon_password.or(existing.rcon_password);
    let verification_enabled = payload
        .verification_enabled
        .unwrap_or(existing.verification_enabled);

    if let Err(e) = sqlx::query(
        "UPDATE servers SET name = $1, ip = $2, port = $3, rcon_password = $4, verification_enabled = $5 WHERE id = $6"
    )
    .bind(&name)
    .bind(&ip)
    .bind(port)
    .bind(&rcon_password)
    .bind(verification_enabled)
    .bind(id)
    .execute(&mut *tx)
    .await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }

    if let Err(e) = tx.commit().await {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }

    let _ = log_admin_action(
        &state.db,
        &user.sub,
        "update_server",
        &format!("ID: {}", id),
        "Updated server",
    )
    .await;

    (StatusCode::OK, Json("Server updated")).into_response()
}

#[utoipa::path(
    delete,
    path = "/api/servers/{id}",
    params(
        ("id" = i64, Path, description = "Server ID")
    ),
    responses(
        (status = 200, description = "Server deleted")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn delete_server(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let result = sqlx::query("DELETE FROM servers WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await;

    match result {
        Ok(_) => {
            let _ = log_admin_action(
                &state.db,
                &user.sub,
                "delete_server",
                &format!("ID: {}", id),
                "Deleted server",
            )
            .await;
            (StatusCode::OK, Json("Server deleted")).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// --- Status Check ---

#[utoipa::path(
    post,
    path = "/api/servers/check",
    request_body = CheckServerRequest,
    responses(
        (status = 200, description = "Connected successfully"),
        (status = 400, description = "Connection failed")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn check_server_status(Json(payload): Json<CheckServerRequest>) -> impl IntoResponse {
    let address = format!("{}:{}", payload.ip, payload.port);

    // Attempt RCON connection
    // Note: rcon crate usage depends on version. rcon 0.6.0 typically:
    // Connection::builder().connect("address", "password").await

    let pwd = payload.rcon_password.unwrap_or_default();

    match check_rcon(&address, &pwd).await {
        Ok(_) => (StatusCode::OK, Json("Connected successfully")).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(format!("Connection failed: {}", e)),
        )
            .into_response(),
    }
}

// --- Player Management ---

use crate::utils::rcon::send_command;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Serialize, utoipa::ToSchema)]
pub struct Player {
    pub userid: i32,
    pub name: String,
    pub steam_id: String,
    pub time: String,
    pub ping: i32,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct KickPlayerRequest {
    pub userid: i32,
    pub reason: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BanPlayerRequest {
    pub userid: i32,
    pub duration: i32, // minutes, 0 = permanent
    pub reason: Option<String>,
}

fn build_unban_commands(steam_id: &str, ip: &str) -> Vec<String> {
    let mut commands = Vec::new();

    let steam_id = steam_id.trim();
    if !steam_id.is_empty() && steam_id != "Unknown" {
        commands.push(format!("sm_unban \"{}\"", steam_id));
    }

    let ip = ip.trim();
    if !ip.is_empty() && ip != "0.0.0.0" {
        commands.push(format!("sm_unban \"{}\"", ip));
    }

    commands
}

async fn rollback_rcon_ban(address: &str, password: &str, steam_id: &str, ip: &str) -> Vec<String> {
    let mut errors = Vec::new();

    for command in build_unban_commands(steam_id, ip) {
        if let Err(error) = send_command(address, password, &command).await {
            errors.push(format!("{}: {}", command, error));
        }
    }

    errors
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/players",
    params(
        ("id" = i64, Path, description = "Server ID")
    ),
    responses(
        (status = 200, description = "List players", body = Vec<Player>),
        (status = 404, description = "Server not found")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn get_server_players(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    // Get server info
    let server = sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    let server = match server {
        Some(s) => s,
        None => return (StatusCode::NOT_FOUND, "Server not found").into_response(),
    };

    let address = format!("{}:{}", server.ip, server.port);
    let pwd = server.rcon_password.unwrap_or_default();

    match send_command(&address, &pwd, "status").await {
        Ok(output) => {
            tracing::info!("RCON 'status' output: \n{}", output); // Debug log

            let mut players = Vec::new();
            // Regex to parse status output
            // Regex: #\s*(\d+)\s+\d+\s+"(.+?)"\s+(\S+)\s+(\S+)\s+(\d+)
            // Output format: # userid slot "name" steamid time ping ...
            let re = Regex::new(r#"#\s+(\d+)\s+\d+\s+"(.+?)"\s+(\S+)\s+(\S+)\s+(\d+)"#).unwrap();

            for cap in re.captures_iter(&output) {
                let userid = cap[1].parse::<i32>().unwrap_or(-1);
                let name = cap[2].to_string();
                let steam_id = cap[3].to_string();
                let time = cap[4].to_string();
                let ping = cap[5].parse::<i32>().unwrap_or(0);

                players.push(Player {
                    userid,
                    name,
                    steam_id,
                    time,
                    ping,
                });
            }

            (StatusCode::OK, Json(players)).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(format!("RCON Error: {}", e))).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/servers/{id}/kick",
    params(
        ("id" = i64, Path, description = "Server ID")
    ),
    request_body = KickPlayerRequest,
    responses(
        (status = 200, description = "Player kicked")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn kick_player(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
    Path(id): Path<i64>,
    Json(payload): Json<KickPlayerRequest>,
) -> impl IntoResponse {
    let server = sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    let server = match server {
        Some(s) => s,
        None => return (StatusCode::NOT_FOUND, "Server not found").into_response(),
    };

    let address = format!("{}:{}", server.ip, server.port);
    let pwd = server.rcon_password.unwrap_or_default();

    // Command: kickid <userid> [reason]
    let reason = payload.reason.unwrap_or("Kicked by admin".to_string());
    let command = format!("kickid {} \"{}\"", payload.userid, reason);

    match send_command(&address, &pwd, &command).await {
        Ok(_) => {
            let _ = log_admin_action(
                &state.db,
                &user.sub,
                "kick_player",
                &format!("Server: {}, UserID: {}", server.name, payload.userid),
                &format!("Reason: {}", reason),
            )
            .await;
            (StatusCode::OK, Json("Player kicked")).into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(format!("Failed to kick: {}", e)),
        )
            .into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/servers/{id}/ban",
    params(
        ("id" = i64, Path, description = "Server ID")
    ),
    request_body = BanPlayerRequest,
    responses(
        (status = 200, description = "Player banned")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn ban_player(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
    Path(id): Path<i64>,
    Json(payload): Json<BanPlayerRequest>,
) -> impl IntoResponse {
    let server = sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    let server = match server {
        Some(s) => s,
        None => return (StatusCode::NOT_FOUND, "Server not found").into_response(),
    };

    let address = format!("{}:{}", server.ip, server.port);
    let pwd = server.rcon_password.unwrap_or_default();

    // 1. Get Player Info from "status"
    // We need SteamID and IP to ban properly in DB
    let player_info = match send_command(&address, &pwd, "status").await {
        Ok(output) => {
            // Try to match specific userid
            // Note: The extended regex attempts to capture IP at the end if present.
            // Standard output: # userid slot "name" steamid time ping loss state rate adr
            // "adr" is usually IP:Port

            // Refined Regex for full line:
            // # 301 1 "Name" STEAM_X:Y:Z ... ... ... ... ... IP:Port
            // Let's use a simpler approach: iterate all, find matching userid

            let mut found = None;
            for line in output.lines() {
                if line.trim().starts_with("#") {
                    let _parts: Vec<&str> = line.split_whitespace().collect();
                    // Parts: #, userid, slot, "Name", SteamID, ...
                    // Because Name can have spaces, splitting by whitespace is risky.
                    // But we have Regex!
                    // Let's use the verified regex from get_players but extend it optionally for IP

                    // Try to parse the specific userid we are banning
                    // Search for "# <userid> "
                    let prefix = format!("# {} ", payload.userid);
                    if line.contains(&prefix) {
                        // Found our guy?
                        // Let's rely on Regex again.
                        // Regex: #\s+<userid>\s+\d+\s+"(.+?)"\s+(\S+)\s+.*\s+(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}:?\d*)
                        let ip_re = Regex::new(&format!(r#"#\s+{}\s+\d+\s+"(.+?)"\s+(\S+)\s+.*\s+(\d{{1,3}}\.\d{{1,3}}\.\d{{1,3}}\.\d{{1,3}})"#, payload.userid)).unwrap();

                        if let Some(cap) = ip_re.captures(line) {
                            found =
                                Some((cap[1].to_string(), cap[2].to_string(), cap[3].to_string()));
                            break;
                        } else {
                            // Fallback if IP not found/parsed (e.g. "loopback" or weird format)
                            // Just get Name/SteamID
                            let basic_re = Regex::new(&format!(
                                r#"#\s+{}\s+\d+\s+"(.+?)"\s+(\S+)"#,
                                payload.userid
                            ))
                            .unwrap();
                            if let Some(cap) = basic_re.captures(line) {
                                found = Some((
                                    cap[1].to_string(),
                                    cap[2].to_string(),
                                    "0.0.0.0".to_string(),
                                ));
                                break;
                            }
                        }
                    }
                }
            }
            found
        }
        Err(_) => None, // RCON failed
    };

    let (name, steam_id, ip) = player_info.unwrap_or((
        "Unknown".to_string(),
        "Unknown".to_string(),
        "0.0.0.0".to_string(),
    ));

    let steam_id_64 = if steam_id != "Unknown" {
        state.steam_service.resolve_steam_id(&steam_id).await
    } else {
        None
    };
    let steam_id_3 = steam_id_64
        .as_deref()
        .and_then(|value| state.steam_service.id64_to_id3(value));

    let expires_at = if payload.duration > 0 {
        Some(chrono::Utc::now() + chrono::Duration::minutes(payload.duration as i64))
    } else {
        None
    };

    let ip_only = ip.split(':').next().unwrap_or(&ip).to_string();
    let reason = payload
        .reason
        .clone()
        .unwrap_or("Banned by admin".to_string());

    tracing::info!(
        "Attempting to ban player: Name={}, SteamID={}, IP={}",
        name,
        steam_id,
        ip_only
    );

    let command = format!(
        "sm_ban #{} {} \"{}\"",
        payload.userid, payload.duration, reason
    );

    if let Err(error) = send_command(&address, &pwd, &command).await {
        return (
            StatusCode::BAD_REQUEST,
            Json(format!("Failed to ban: {}", error)),
        )
            .into_response();
    }

    let db_result = sqlx::query(
        "INSERT INTO bans (name, steam_id, steam_id_3, steam_id_64, ip, ban_type, reason, duration, admin_name, expires_at, created_at, status, server_id) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW(), 'active', $11)"
    )
    .bind(&name)
    .bind(&steam_id)
    .bind(&steam_id_3)
    .bind(&steam_id_64)
    .bind(&ip_only)
    .bind("ip")
    .bind(&reason)
    .bind(payload.duration.to_string())
    .bind(&user.sub)
    .bind(expires_at)
    .bind(server.id)
    .execute(&state.db)
    .await;

    match db_result {
        Ok(_) => {
            let _ = log_admin_action(
                &state.db,
                &user.sub,
                "ban_player_rcon_db",
                &format!("Server: {}, UserID: {}", server.name, payload.userid),
                &format!(
                    "Duration: {}, Reason: {}, Player: {} ({})",
                    payload.duration, reason, name, steam_id
                ),
            )
            .await;
            (StatusCode::OK, Json("Player banned and recorded")).into_response()
        }
        Err(error) => {
            tracing::error!("Failed to insert ban into DB after RCON success: {}", error);

            let rollback_errors = rollback_rcon_ban(&address, &pwd, &steam_id, &ip_only).await;
            if rollback_errors.is_empty() {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json("Ban applied in game but database write failed; rollback completed"),
                )
                    .into_response();
            }

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(format!(
                    "Ban applied in game but database write failed; rollback also failed: {}",
                    rollback_errors.join("; ")
                )),
            )
                .into_response()
        }
    }
}
