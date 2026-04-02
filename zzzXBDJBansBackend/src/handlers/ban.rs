use crate::handlers::auth::Claims;
use crate::models::ban::{
    Ban, BanListQuery, CreateBanRequest, PaginatedBanResponse, PaginatedPublicBanResponse,
    PublicBan, PublicBanListQuery, UpdateBanRequest,
};
use crate::models::server::Server;
use crate::utils::{calculate_expires_at, log_admin_action};
use crate::AppState;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct BanFilter {
    steam_id: Option<String>,
    ip: Option<String>,
}

const EFFECTIVE_BAN_STATUS_SQL: &str = "CASE
        WHEN b.status = 'active' AND b.expires_at IS NOT NULL AND b.expires_at < NOW() THEN 'expired'
        ELSE b.status
    END";

fn normalize_public_status_filter(status: Option<&str>) -> Option<String> {
    let status = status?.trim().to_lowercase();
    if status.is_empty() || status == "all" {
        None
    } else {
        Some(status)
    }
}

fn normalize_search_filter(search: Option<&str>) -> Option<String> {
    let search = search?.trim();
    if search.is_empty() {
        None
    } else {
        Some(search.to_string())
    }
}

#[utoipa::path(
    get,
    path = "/api/bans",
    params(
        ("page" = Option<i64>, Query, description = "Page number, starts at 1"),
        ("page_size" = Option<i64>, Query, description = "Page size, max 100")
    ),
    responses(
        (status = 200, description = "List bans", body = PaginatedBanResponse)
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn list_bans(
    State(state): State<Arc<AppState>>,
    Query(query): Query<BanListQuery>,
) -> impl IntoResponse {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(25).clamp(1, 100);
    let offset = (page - 1) * page_size;

    let total = match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM bans")
        .fetch_one(&state.db)
        .await
    {
        Ok(total) => total,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let bans = sqlx::query_as::<_, Ban>(
        r#"SELECT
            b.id,
            b.name,
            b.steam_id,
            b.steam_id_3,
            b.steam_id_64,
            b.ip,
            b.ban_type,
            b.reason,
            b.duration,
            CASE
                WHEN b.status = 'active' AND b.expires_at IS NOT NULL AND b.expires_at < NOW() THEN 'expired'
                ELSE b.status
            END AS status,
            b.admin_name,
            b.created_at,
            b.expires_at,
            b.server_id
        FROM bans b
        ORDER BY b.created_at DESC
        LIMIT $1 OFFSET $2"#
    )
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.db)
        .await;

    match bans {
        Ok(data) => {
            let response = PaginatedBanResponse {
                items: data,
                total,
                page,
                page_size,
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/bans/public",
    params(
        ("page" = Option<i64>, Query, description = "Page number, starts at 1"),
        ("page_size" = Option<i64>, Query, description = "Page size, max 100"),
        ("status" = Option<String>, Query, description = "Status filter, e.g. active/expired/all"),
        ("search" = Option<String>, Query, description = "Search by player name or Steam ID")
    ),
    responses(
        (status = 200, description = "List public bans", body = PaginatedPublicBanResponse)
    )
)]
pub async fn list_public_bans(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PublicBanListQuery>,
) -> impl IntoResponse {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(25).clamp(1, 100);
    let offset = (page - 1) * page_size;
    let status_filter = normalize_public_status_filter(query.status.as_deref());
    let search_filter = normalize_search_filter(query.search.as_deref());

    let count_sql = format!(
        "SELECT COUNT(*)
         FROM bans b
         WHERE ($1::text IS NULL OR {status_sql} = $1)
           AND (
               $2::text IS NULL
               OR b.name ILIKE '%' || $2 || '%'
               OR b.steam_id ILIKE '%' || $2 || '%'
               OR COALESCE(b.steam_id_64, '') ILIKE '%' || $2 || '%'
               OR COALESCE(b.steam_id_3, '') ILIKE '%' || $2 || '%'
           )",
        status_sql = EFFECTIVE_BAN_STATUS_SQL
    );

    let total = match sqlx::query_scalar::<_, i64>(&count_sql)
        .bind(status_filter.as_deref())
        .bind(search_filter.as_deref())
        .fetch_one(&state.db)
        .await
    {
        Ok(total) => total,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let list_sql = format!(
        "SELECT
            b.id,
            b.name,
            b.steam_id,
            b.steam_id_3,
            b.steam_id_64,
            b.reason,
            b.duration,
            {status_sql} AS status,
            b.admin_name,
            b.created_at,
            b.expires_at
         FROM bans b
         WHERE ($1::text IS NULL OR {status_sql} = $1)
           AND (
               $2::text IS NULL
               OR b.name ILIKE '%' || $2 || '%'
               OR b.steam_id ILIKE '%' || $2 || '%'
               OR COALESCE(b.steam_id_64, '') ILIKE '%' || $2 || '%'
               OR COALESCE(b.steam_id_3, '') ILIKE '%' || $2 || '%'
           )
         ORDER BY b.created_at DESC
         LIMIT $3 OFFSET $4",
        status_sql = EFFECTIVE_BAN_STATUS_SQL
    );

    let bans = sqlx::query_as::<_, PublicBan>(&list_sql)
        .bind(status_filter.as_deref())
        .bind(search_filter.as_deref())
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.db)
        .await;

    match bans {
        Ok(items) => {
            let response = PaginatedPublicBanResponse {
                items,
                total,
                page,
                page_size,
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

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
        let steam_service = state.steam_service.as_ref();
        if let Some(id64) = steam_service.resolve_steam_id(&steam_id).await {
            steam_id_64 = id64;
        }
    }

    // 1. Check for DIRECT Account Ban (优先使用 steam_id_64 匹配)
    let account_ban = if !steam_id_64.is_empty() {
        sqlx::query_as::<_, Ban>(
            "SELECT * FROM bans WHERE status = 'active' AND (expires_at IS NULL OR expires_at > NOW()) AND (steam_id_64 = $1 OR steam_id = $2) LIMIT 1"
        )
        .bind(&steam_id_64)
        .bind(&steam_id)
        .fetch_optional(&state.db)
        .await
    } else {
        sqlx::query_as::<_, Ban>(
            "SELECT * FROM bans WHERE status = 'active' AND (expires_at IS NULL OR expires_at > NOW()) AND steam_id = $1 LIMIT 1"
        )
        .bind(&steam_id)
        .fetch_optional(&state.db)
        .await
    };

    match account_ban {
        Ok(Some(b)) => {
            return (StatusCode::OK, Json(b)).into_response();
        }
        Err(e) => {
            tracing::error!("CHECK_BAN: DB Error on Account Check: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
        Ok(None) => {}
    }

    // 2. Check for IP Ban (Matches IP AND ban_type = 'ip')

    let ip_ban = sqlx::query_as::<_, Ban>(
        "SELECT * FROM bans WHERE status = 'active' AND (expires_at IS NULL OR expires_at > NOW()) AND ip = $1 AND ban_type = 'ip' LIMIT 1"
    )
    .bind(&ip)
    .fetch_optional(&state.db)
    .await;

    match ip_ban {
        Ok(Some(b)) => {
            return (StatusCode::OK, Json(b)).into_response();
        }
        Ok(None) => {
            return (StatusCode::NOT_FOUND, Json("Not banned")).into_response();
        }
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
    State(state): State<Arc<AppState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let steam_id = params.get("steam_id");
    if steam_id.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Missing steam_id" })),
        )
            .into_response();
    }
    let steam_id = steam_id.unwrap();

    let data = state.global_ban_service.get_ban(steam_id).await;
    (StatusCode::OK, Json(data)).into_response()
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
    let map = state.global_ban_service.get_bans(payload.steam_ids).await;
    (StatusCode::OK, Json(map)).into_response()
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
    let resolved_ids = resolve_ban_steam_identifiers(&state, &payload.steam_id).await;

    let result = sqlx::query_as::<_, Ban>(
        "INSERT INTO bans (name, steam_id, steam_id_3, steam_id_64, ip, ban_type, reason, duration, admin_name, expires_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING *"
    )
    .bind(&payload.name)
    .bind(&resolved_ids.steam_id)
    .bind(&resolved_ids.steam_id_3)
    .bind(&resolved_ids.steam_id_64)
    .bind(&payload.ip)
    .bind(&payload.ban_type)
    .bind(&payload.reason)
    .bind(&payload.duration)
    .bind(&user.sub)
    .bind(expires_at)
    .fetch_one(&state.db)
    .await;

    match result {
        Ok(created_ban) => {
            let _ = log_admin_action(
                &state.db,
                &user.sub,
                "create_ban",
                &format!(
                    "User: {}, SteamID64: {}",
                    payload.name,
                    resolved_ids
                        .steam_id_64
                        .clone()
                        .unwrap_or_else(|| resolved_ids.steam_id.clone())
                ),
                &format!(
                    "Reason: {}, Duration: {}",
                    payload.reason.clone().unwrap_or_default(),
                    payload.duration
                ),
            )
            .await;
            (StatusCode::CREATED, Json(created_ban)).into_response()
        }
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
    let current = match sqlx::query_as::<_, Ban>("SELECT * FROM bans WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(ban)) => ban,
        Ok(None) => return (StatusCode::NOT_FOUND, Json("Ban not found")).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let name = payload.name.unwrap_or(current.name);
    let resolved_ids = match payload.steam_id {
        Some(steam_id) => resolve_ban_steam_identifiers(&state, &steam_id).await,
        None => ResolvedBanSteamIds {
            steam_id: current.steam_id.clone(),
            steam_id_3: current.steam_id_3.clone(),
            steam_id_64: current.steam_id_64.clone(),
        },
    };
    let ip = payload.ip.unwrap_or(current.ip);
    let ban_type = payload.ban_type.unwrap_or(current.ban_type);
    let reason = payload.reason.or(current.reason);
    let (duration, expires_at) = match payload.duration {
        Some(duration) if duration != current.duration => {
            let expires_at = calculate_expires_at(&duration);
            (duration, expires_at)
        }
        Some(duration) => (duration, current.expires_at),
        None => (current.duration, current.expires_at),
    };
    let status = payload.status.unwrap_or(current.status);

    let updated_ban = match sqlx::query_as::<_, Ban>(
        "UPDATE bans
         SET name = $1,
             steam_id = $2,
             steam_id_3 = $3,
             steam_id_64 = $4,
             ip = $5,
             ban_type = $6,
             reason = $7,
             duration = $8,
             status = $9,
             expires_at = $10
         WHERE id = $11
         RETURNING *"
    )
    .bind(&name)
    .bind(&resolved_ids.steam_id)
    .bind(&resolved_ids.steam_id_3)
    .bind(&resolved_ids.steam_id_64)
    .bind(&ip)
    .bind(&ban_type)
    .bind(&reason)
    .bind(&duration)
    .bind(&status)
    .bind(expires_at)
    .bind(id)
    .fetch_one(&state.db)
    .await
    {
        Ok(ban) => ban,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let _ = log_admin_action(
        &state.db,
        &user.sub,
        "update_ban",
        &format!("BanID: {}", id),
        "Updated ban details",
    )
    .await;

    (StatusCode::OK, Json(updated_ban)).into_response()
}

struct ResolvedBanSteamIds {
    steam_id: String,
    steam_id_3: Option<String>,
    steam_id_64: Option<String>,
}

fn unban_commands_for_ban(ban: &Ban) -> Vec<String> {
    let mut commands = Vec::new();

    let steam_id = ban.steam_id.trim();
    if !steam_id.is_empty() {
        commands.push(format!("sm_unban \"{}\"", steam_id));
    }

    let ip = ban.ip.trim();
    if !ip.is_empty() {
        commands.push(format!("sm_unban \"{}\"", ip));
    }

    commands
}

async fn unban_ban_on_servers(ban: &Ban, servers: &[Server]) -> Vec<String> {
    let commands = unban_commands_for_ban(ban);
    if commands.is_empty() {
        return Vec::new();
    }

    let mut failures = Vec::new();

    for server in servers {
        let address = format!("{}:{}", server.ip, server.port);
        let password = server.rcon_password.clone().unwrap_or_default();

        for command in &commands {
            if let Err(error) = crate::utils::rcon::send_command(&address, &password, command).await {
                failures.push(format!("{} ({}) -> {}", server.name, command, error));
            }
        }
    }

    failures
}

async fn resolve_ban_steam_identifiers(
    state: &Arc<AppState>,
    steam_id_input: &str,
) -> ResolvedBanSteamIds {
    let steam_id_input = steam_id_input.trim();
    let steam_service = state.steam_service.as_ref();

    match steam_service.resolve_steam_id(steam_id_input).await {
        Some(steam_id_64) => ResolvedBanSteamIds {
            steam_id: steam_service
                .id64_to_id2(&steam_id_64)
                .unwrap_or_else(|| steam_id_input.to_string()),
            steam_id_3: steam_service.id64_to_id3(&steam_id_64),
            steam_id_64: Some(steam_id_64),
        },
        None => ResolvedBanSteamIds {
            steam_id: steam_id_input.to_string(),
            steam_id_3: None,
            steam_id_64: None,
        },
    }
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
    tracing::info!(
        "DELETE /api/bans/{} requested by user: {}, role: {}",
        id,
        user.sub,
        user.role
    );

    // 1. Permission Check
    if user.role != "super_admin" {
        tracing::warn!("Permission denied for user {}", user.sub);
        return (
            StatusCode::FORBIDDEN,
            Json("Only super admins can delete bans"),
        )
            .into_response();
    }

    // 2. Fetch Ban Details (for RCON unban)
    // Removed unwrap_or(None) to see actual error if mapping fails
    let ban_query = sqlx::query_as::<_, Ban>("SELECT * FROM bans WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await;

    let ban = match ban_query {
        Ok(Some(b)) => b,
        Ok(None) => {
            tracing::warn!("Ban ID {} not found in DB", id);
            return (StatusCode::NOT_FOUND, "Ban not found").into_response();
        }
        Err(e) => {
            tracing::error!("DB Error fetching ban {}: {}", id, e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("DB Error: {}", e),
            )
                .into_response();
        }
    };

    let servers = match sqlx::query_as::<_, Server>("SELECT * FROM servers")
        .fetch_all(&state.db)
        .await
    {
        Ok(servers) => servers,
        Err(e) => {
            tracing::error!("Failed to fetch servers for unban {}: {}", id, e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(format!("Failed to load servers for unban: {}", e)),
            )
                .into_response();
        }
    };

    let unban_failures = unban_ban_on_servers(&ban, &servers).await;
    if !unban_failures.is_empty() {
        tracing::error!("Failed to unban ban {} on some servers: {:?}", id, unban_failures);
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "error": "Failed to unban on all servers; database record was kept",
                "details": unban_failures,
            })),
        )
            .into_response();
    }

    let result = sqlx::query("DELETE FROM bans WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await;

    match result {
        Ok(res) => {
            if res.rows_affected() == 0 {
                tracing::warn!("DELETE executed but 0 rows affected for ID {}", id);
            }

            let _ = log_admin_action(
                &state.db,
                &user.sub,
                "delete_ban",
                &format!("BanID: {}, Target: {} ({})", id, ban.name, ban.steam_id),
                "Deleted ban after successful unban",
            )
            .await;
            (
                StatusCode::OK,
                Json("Ban deleted and unbanned on all servers"),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to delete ban from DB: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
