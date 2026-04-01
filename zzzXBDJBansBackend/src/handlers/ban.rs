use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use crate::AppState;
use crate::models::ban::{Ban, PublicBan, CreateBanRequest, UpdateBanRequest};
use crate::handlers::auth::Claims;
use crate::utils::{log_admin_action, calculate_expires_at};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
pub struct BanFilter {
    steam_id: Option<String>,
    ip: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/bans",
    responses(
        (status = 200, description = "List all bans", body = Vec<Ban>)
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn list_bans(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    // Lazy expire check: Update all active bans that have expired
    let _ = sqlx::query("UPDATE bans SET status = 'expired' WHERE status = 'active' AND expires_at < NOW()")
        .execute(&state.db)
        .await;

    let bans = sqlx::query_as::<_, Ban>("SELECT * FROM bans ORDER BY created_at DESC")
        .fetch_all(&state.db)
        .await;

    match bans {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/bans/public",
    responses(
        (status = 200, description = "List public bans", body = Vec<PublicBan>)
    )
)]
pub async fn list_public_bans(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    // Lazy expire check: Update all active bans that have expired
    let _ = sqlx::query("UPDATE bans SET status = 'expired' WHERE status = 'active' AND expires_at < NOW()")
        .execute(&state.db)
        .await;

    // Select specific columns to avoid exposing IP
    let bans = sqlx::query_as::<_, PublicBan>(
        "SELECT id, name, steam_id, steam_id_3, steam_id_64, reason, duration, status, admin_name, created_at, expires_at FROM bans ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await;

    match bans {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ... imports
use crate::services::steam_api::SteamService;

// ... check_ban
#[utoipa::path(
    get,
    path = "/api/check_ban",
    params(
        ("steam_id" = Option<String>, Query, description = "SteamID to check"),
        ("ip" = Option<String>, Query, description = "IP to check")
    ),
    responses(
        (status = 200, description = "Ban details if banned", body = Ban),
        (status = 404, description = "Not banned")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn check_ban(
    State(state): State<Arc<AppState>>,
    Query(params): Query<BanFilter>,
) -> impl IntoResponse {
    if params.steam_id.is_none() && params.ip.is_none() {
        return (StatusCode::BAD_REQUEST, "Missing steam_id or ip").into_response();
    }
    
    let steam_id = params.steam_id.unwrap_or_default();
    let ip = params.ip.unwrap_or_default();

    // CONVERSION: Ensure SteamID is in standard SteamID2 format (STEAM_0:...) for DB lookup
    // 将输入的 SteamID 转换为 steam_id_64 格式进行匹配
    let mut steam_id_64 = String::new();
    if !steam_id.is_empty() {
        let steam_service = SteamService::new();
        if let Some(id64) = steam_service.resolve_steam_id(&steam_id).await {
            steam_id_64 = id64;
        }
    }
    
    // 1. Check for DIRECT Account Ban (优先使用 steam_id_64 匹配)
    let account_ban = if !steam_id_64.is_empty() {
        sqlx::query_as::<_, Ban>(
            "SELECT * FROM bans WHERE status = 'active' AND (steam_id_64 = ? OR steam_id = ?) LIMIT 1"
        )
        .bind(&steam_id_64)
        .bind(&steam_id)
        .fetch_optional(&state.db)
        .await
    } else {
        sqlx::query_as::<_, Ban>(
            "SELECT * FROM bans WHERE status = 'active' AND steam_id = ? LIMIT 1"
        )
        .bind(&steam_id)
        .fetch_optional(&state.db)
        .await
    };

    match account_ban {
        Ok(Some(b)) => {

            // Check expiration
            if let Some(expires_at) = b.expires_at {
                if Utc::now() > expires_at {

                    let _ = sqlx::query("UPDATE bans SET status = 'expired' WHERE id = ?")
                        .bind(b.id).execute(&state.db).await;
                    // Expired - Do NOT return yet. Treat as not banned, proceed to check IP.
                } else {
                    return (StatusCode::OK, Json(b)).into_response();
                }
            } else {
                return (StatusCode::OK, Json(b)).into_response();
            }
        },
        Err(e) => {
             tracing::error!("CHECK_BAN: DB Error on Account Check: {}", e);
             return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        },
        Ok(None) => {

        }
    }

    // 2. Check for IP Ban (Matches IP AND ban_type = 'ip')

    let ip_ban = sqlx::query_as::<_, Ban>(
        "SELECT * FROM bans WHERE status = 'active' AND ip = ? AND ban_type = 'ip' LIMIT 1"
    )
    .bind(&ip)
    .fetch_optional(&state.db)
    .await;

    match ip_ban {
        Ok(Some(b)) => {

             // Check expiration for the IP ban
            if let Some(expires_at) = b.expires_at {
                if Utc::now() > expires_at {

                    let _ = sqlx::query("UPDATE bans SET status = 'expired' WHERE id = ?")
                        .bind(b.id).execute(&state.db).await;
                    return (StatusCode::NOT_FOUND, Json("Not banned (Expired)")).into_response();
                }
            }

            // HIT! IP is banned, and user has no personal ban.
            tracing::info!("CHECK_BAN: IP Ban Hit for new identity! Triggering Auto-Ban. IP: {}, New SteamID: {}", ip, steam_id);

            // Create NEW Ban Record
            let reason = "同IP关联封禁 (Different account repeated IP login)".to_string();
            // Inherit expiration from the parent IP ban
            let expires_at = b.expires_at; 
            
            let insert_result = sqlx::query(
                "INSERT INTO bans (name, steam_id, ip, ban_type, reason, duration, admin_name, expires_at, created_at, status, server_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, NOW(), 'active', ?)"
            )
            .bind("Auto-Banned") 
            .bind(&steam_id)
            .bind(&ip)
            .bind("account") 
            .bind(&reason)
            .bind(&b.duration) 
            .bind("System (IP Match)")
            .bind(expires_at)
            .bind(b.server_id)
            .execute(&state.db)
            .await;

            match insert_result {
                Ok(res) => {
                    let new_id = res.last_insert_id() as i64;
                    tracing::info!("CHECK_BAN: Auto-Ban Created Successfully. New ID: {}", new_id);
                    let new_ban = Ban {
                        id: new_id,
                        name: "Auto-Banned".to_string(),
                        steam_id: steam_id,
                        steam_id_3: None,
                        steam_id_64: Some(steam_id_64.clone()),
                        ip: ip,
                        ban_type: "account".to_string(),
                        reason: Some(reason),
                        duration: b.duration,
                        status: "active".to_string(),
                        admin_name: Some("System (IP Match)".to_string()),
                        created_at: Some(Utc::now()),
                        expires_at: expires_at,
                        server_id: b.server_id
                    };
                    return (StatusCode::OK, Json(new_ban)).into_response();
                },
                Err(e) => {
                    tracing::error!("CHECK_BAN: Failed to auto-create ban: {}", e);
                    // If insert fails, still return the IP ban so they are blocked
                    return (StatusCode::OK, Json(b)).into_response();
                }
            }
        },
        Ok(None) => {

            return (StatusCode::NOT_FOUND, Json("Not banned")).into_response();
        },
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// 代理查询 GOKZ 全局封禁
#[utoipa::path(
    get,
    path = "/api/check_global_ban",
    params(
        ("steam_id" = String, Query, description = "SteamID to check (ID64)")
    ),
    responses(
        (status = 200, description = "Global ban details or null"),
        (status = 400, description = "Missing steam_id")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn check_global_ban(
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let steam_id = params.get("steam_id");
    if steam_id.is_none() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing steam_id" }))).into_response();
    }
    let steam_id = steam_id.unwrap();

    // Proxy request to GOKZ API
    let url = format!("https://api.gokz.top/api/v1/bans?steamid64={}", steam_id);
    match reqwest::get(&url).await {
        Ok(resp) => {
            if resp.status().is_success() {
                match resp.json::<serde_json::Value>().await {
                    Ok(data) => (StatusCode::OK, Json(data)).into_response(),
                    Err(e) => {
                        tracing::error!("Failed to parse GOKZ API response: {}", e);
                         (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Failed to parse external API" }))).into_response()
                    }
                }
            } else {
                 (resp.status(), Json(json!({ "error": "External API error" }))).into_response()
            }
        },
        Err(e) => {
            tracing::error!("Failed to call GOKZ API: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Failed to call external API" }))).into_response()
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BulkBanCheckRequest {
    pub steam_ids: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/api/check_global_ban/bulk",
    request_body = BulkBanCheckRequest,
    responses(
        (status = 200, description = "Bulk ban details", body = std::collections::HashMap<String, Option<serde_json::Value>>)
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn check_global_ban_bulk(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<BulkBanCheckRequest>,
) -> impl IntoResponse {
    let client = state.client.clone();
    let unique_ids: Vec<String> = payload.steam_ids.into_iter()
        .filter(|id| !id.is_empty())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let fetched_results = fetch_all_bans(unique_ids, client).await;
    
    let mut map = std::collections::HashMap::new();
    for (id, res) in fetched_results {
         map.insert(id, res);
    }

    (StatusCode::OK, Json(map)).into_response()
}

async fn fetch_all_bans(ids: Vec<String>, client: reqwest::Client) -> Vec<(String, Option<serde_json::Value>)> {
    let mut tasks = Vec::new();
    for id in ids {
        let client = client.clone();
        tasks.push(tokio::spawn(async move {
            let url = format!("https://api.gokz.top/api/v1/bans?steamid64={}", id);
            match client.get(&url).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<serde_json::Value>().await {
                            Ok(data) => (id, Some(data)),
                            Err(_) => (id, None),
                        }
                    } else {
                        (id, None)
                    }
                },
                Err(_) => (id, None),
            }
        }));
    }

    let mut results = Vec::new();
    for task in tasks {
        if let Ok(res) = task.await {
            results.push(res);
        }
    }
    results
}

#[utoipa::path(
    post,
    path = "/api/bans",
    request_body = CreateBanRequest,
    responses(
        (status = 201, description = "Ban created"),
        (status = 400, description = "Bad request")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn create_ban(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
    Json(payload): Json<CreateBanRequest>,
) -> impl IntoResponse {
    let expires_at = calculate_expires_at(&payload.duration);

    // 解析输入的 SteamID 为各种格式
    let steam_service = SteamService::new();
    let steam_id_64 = steam_service.resolve_steam_id(&payload.steam_id).await
        .unwrap_or_else(|| payload.steam_id.clone());
    
    let steam_id_2 = steam_service.id64_to_id2(&steam_id_64)
        .unwrap_or_else(|| payload.steam_id.clone());
    
    let steam_id_3 = steam_service.id64_to_id3(&steam_id_64)
        .unwrap_or_default();

    let result = sqlx::query(
        "INSERT INTO bans (name, steam_id, steam_id_3, steam_id_64, ip, ban_type, reason, duration, admin_name, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&payload.name)
    .bind(&steam_id_2)
    .bind(&steam_id_3)
    .bind(&steam_id_64)
    .bind(&payload.ip)
    .bind(&payload.ban_type)
    .bind(&payload.reason)
    .bind(&payload.duration)
    .bind(&payload.admin_name)
    .bind(expires_at)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => {
            let _ = log_admin_action(
                &state.db, 
                &user.sub, 
                "create_ban", 
                &format!("User: {}, SteamID64: {}", payload.name, steam_id_64), 
                &format!("Reason: {}, Duration: {}", payload.reason.clone().unwrap_or_default(), payload.duration)
            ).await;
            (StatusCode::CREATED, Json("Ban created")).into_response()
        },
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    put,
    path = "/api/bans/{id}",
    params(
        ("id" = i64, Path, description = "Ban ID")
    ),
    request_body = UpdateBanRequest,
    responses(
        (status = 200, description = "Ban updated"),
        (status = 404, description = "Ban not found")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn update_ban(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateBanRequest>,
) -> impl IntoResponse {
    if let Some(status) = payload.status {
        let _ = sqlx::query("UPDATE bans SET status = ? WHERE id = ?")
            .bind(status).bind(id)
            .execute(&state.db).await;
    }
    // ... (other fields name, steam_id etc same as before)
    if let Some(name) = payload.name {
         let _ = sqlx::query("UPDATE bans SET name = ? WHERE id = ?")
            .bind(name).bind(id)
            .execute(&state.db).await;
    }
    if let Some(steam_id) = payload.steam_id {
         let _ = sqlx::query("UPDATE bans SET steam_id = ? WHERE id = ?")
            .bind(steam_id).bind(id)
            .execute(&state.db).await;
    }
    if let Some(ip) = payload.ip {
         let _ = sqlx::query("UPDATE bans SET ip = ? WHERE id = ?")
            .bind(ip).bind(id)
            .execute(&state.db).await;
    }
    if let Some(ban_type) = payload.ban_type {
         let _ = sqlx::query("UPDATE bans SET ban_type = ? WHERE id = ?")
            .bind(ban_type).bind(id)
            .execute(&state.db).await;
    }
    if let Some(reason) = payload.reason {
         let _ = sqlx::query("UPDATE bans SET reason = ? WHERE id = ?")
            .bind(reason).bind(id)
            .execute(&state.db).await;
    }
    if let Some(duration) = payload.duration {
         let expires_at = calculate_expires_at(&duration);
         let _ = sqlx::query("UPDATE bans SET duration = ?, expires_at = ? WHERE id = ?")
            .bind(duration).bind(expires_at).bind(id)
            .execute(&state.db).await;
    }

    let _ = log_admin_action(
        &state.db,
        &user.sub,
        "update_ban",
        &format!("BanID: {}", id),
        "Updated ban details"
    ).await;

    (StatusCode::OK, Json("Ban updated")).into_response()
}

#[utoipa::path(
    delete,
    path = "/api/bans/{id}",
    params(
        ("id" = i64, Path, description = "Ban ID")
    ),
    responses(
        (status = 200, description = "Ban deleted"),
        (status = 404, description = "Ban not found"),
        (status = 403, description = "Forbidden")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn delete_ban(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    tracing::info!("DELETE /api/bans/{} requested by user: {}, role: {}", id, user.sub, user.role);

    // 1. Permission Check
    if user.role != "super_admin" {
        tracing::warn!("Permission denied for user {}", user.sub);
        return (StatusCode::FORBIDDEN, Json("Only super admins can delete bans")).into_response();
    }

    // 2. Fetch Ban Details (for RCON unban)
    // Removed unwrap_or(None) to see actual error if mapping fails
    let ban_query = sqlx::query_as::<_, Ban>("SELECT * FROM bans WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db)
        .await;

    let ban = match ban_query {
        Ok(Some(b)) => b,
        Ok(None) => {
             tracing::warn!("Ban ID {} not found in DB", id);
             return (StatusCode::NOT_FOUND, "Ban not found").into_response();
        },
        Err(e) => {
             tracing::error!("DB Error fetching ban {}: {}", id, e);
             return (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)).into_response();
        }
    };



    // 3. Delete from DB first (for fast response)

    let result = sqlx::query("DELETE FROM bans WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await;

    match result {
        Ok(res) => {
            if res.rows_affected() == 0 {
                tracing::warn!("DELETE executed but 0 rows affected for ID {}", id);
            } else {
                // 4. Spawn RCON Unban task (Fire-and-forget)
                // Fetch servers inside the handler first to avoid lifetime issues or clone valid data
                let servers_result = sqlx::query_as::<_, crate::models::server::Server>("SELECT * FROM servers")
                    .fetch_all(&state.db)
                    .await;

                if let Ok(servers) = servers_result {
                    let _ban_clone = ban.clone(); // Ban struct needs simple Clone derive or manual clone
                    // If Ban doesn't implement Clone, we might need to construct a lightweight struct or ensure it does.
                    // Assuming Ban implements Clone (it normally derives FromRow, Debug, Serialize, Deserialize - let's check or just clone fields)
                    // Let's manually reconstruct or assume Clone if easy. 
                    // Actually, let's just use the data we need: steam_id and ip.
                    let steam_id = ban.steam_id.clone();
                    let ip = ban.ip.clone();
                    let ban_name = ban.name.clone();

                    tokio::spawn(async move {
                        tracing::debug!("Background task: Sending unban commands to {} servers for {}", servers.len(), ban_name);
                        use crate::utils::rcon::send_command;
                        use futures::future::join_all;
                        
                        let tasks: Vec<_> = servers.into_iter().map(|server| {
                            let steam_id = steam_id.clone();
                            let ip = ip.clone();
                            async move {
                                let address = format!("{}:{}", server.ip, server.port);
                                let pwd = server.rcon_password.unwrap_or_default();
                                
                                // Unban SteamID
                                if !steam_id.is_empty() {
                                    let cmd = format!("sm_unban \"{}\"", steam_id);
                                    let _ = send_command(&address, &pwd, &cmd).await;
                                }
                                
                                // Unban IP
                                if !ip.is_empty() {
                                    let cmd = format!("sm_unban \"{}\"", ip);
                                    let _ = send_command(&address, &pwd, &cmd).await;
                                }
                            }
                        }).collect();

                        join_all(tasks).await;
                    });
                }
            }

            let _ = log_admin_action(
                &state.db,
                &user.sub,
                "delete_ban",
                &format!("BanID: {}, Target: {} ({})", id, ban.name, ban.steam_id),
                "Deleted ban (Unban commands queued)"
            ).await;
            (StatusCode::OK, Json("Ban deleted, unban process started in background")).into_response()
        },
        Err(e) => {
            tracing::error!("Failed to delete ban from DB: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        },
    }
}
