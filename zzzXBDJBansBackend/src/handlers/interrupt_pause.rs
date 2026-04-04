use std::{env, sync::Arc};

use axum::{
    extract::{Extension, Form, Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use sqlx::FromRow;

use crate::{
    handlers::auth::Claims,
    models::interrupt_pause::{
        InterruptPauseSnapshot, PluginInterruptPauseLookupRequest, PluginInterruptPauseSaveRequest,
        RejectInterruptPauseRequest,
    },
    utils::log_admin_action,
    AppState,
};

const PLUGIN_TOKEN_HEADER: &str = "x-plugin-token";

#[allow(dead_code)]
#[derive(Debug, FromRow)]
struct PluginInterruptPauseSnapshotRow {
    id: i64,
    server_id: i64,
    auth_primary: String,
    auth_steamid64: Option<String>,
    auth_steam3: Option<String>,
    auth_steam2: Option<String>,
    auth_engine: Option<String>,
    player_name: String,
    ip_address: String,
    map_name: String,
    mode: i32,
    course: i32,
    time_seconds: f64,
    checkpoint_count: i32,
    teleport_count: i32,
    storage_version: i32,
    payload: String,
    restore_status: String,
    restore_requested_at: Option<chrono::DateTime<chrono::Utc>>,
    reviewed_at: Option<chrono::DateTime<chrono::Utc>>,
    reviewed_by: Option<String>,
    reject_reason: Option<String>,
    restored_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[utoipa::path(
    get,
    path = "/api/interrupt-pause",
    responses(
        (status = 200, description = "List interrupt pause snapshots", body = Vec<InterruptPauseSnapshot>)
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn list_interrupt_pause_snapshots(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, InterruptPauseSnapshot>(
        "SELECT
            ips.id,
            ips.server_id,
            s.name AS server_name,
            ips.auth_primary,
            ips.auth_steamid64,
            ips.auth_steam3,
            ips.auth_steam2,
            ips.auth_engine,
            ips.player_name,
            ips.ip_address,
            ips.map_name,
            ips.mode,
            ips.course,
            ips.time_seconds,
            ips.checkpoint_count,
            ips.teleport_count,
            ips.storage_version,
            ips.restore_status,
            ips.restore_requested_at,
            ips.reviewed_at,
            ips.reviewed_by,
            ips.reject_reason,
            ips.restored_at,
            ips.created_at,
            ips.updated_at
         FROM interrupt_pause_snapshots ips
         INNER JOIN servers s ON s.id = ips.server_id
         ORDER BY
            CASE ips.restore_status
                WHEN 'pending' THEN 0
                WHEN 'approved' THEN 1
                WHEN 'rejected' THEN 2
                WHEN 'none' THEN 3
                WHEN 'restored' THEN 4
                WHEN 'aborted' THEN 5
                ELSE 6
            END,
            ips.updated_at DESC",
    )
    .fetch_all(&state.db)
    .await;

    match rows {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => {
            tracing::error!("Failed to list interrupt pause snapshots: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "加载中断暂停列表失败" })),
            )
                .into_response()
        }
    }
}

#[utoipa::path(
    put,
    path = "/api/interrupt-pause/{id}/approve",
    params(
        ("id" = i64, Path, description = "Interrupt pause snapshot ID")
    ),
    responses(
        (status = 200, description = "Restore approved")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn approve_interrupt_pause_snapshot(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let result = sqlx::query(
        "UPDATE interrupt_pause_snapshots
         SET restore_status = 'approved',
             reviewed_at = NOW(),
             reviewed_by = $1,
             reject_reason = NULL,
             updated_at = NOW()
         WHERE id = $2",
    )
    .bind(&user.sub)
    .bind(id)
    .execute(&state.db)
    .await;

    match result {
        Ok(done) if done.rows_affected() == 0 => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "记录不存在" })),
        )
            .into_response(),
        Ok(_) => {
            let _ = log_admin_action(
                &state.db,
                &user.sub,
                "approve_interrupt_pause",
                &format!("snapshot:{}", id),
                "Approved interrupt pause restore request",
            )
            .await;

            (StatusCode::OK, Json(json!({ "message": "已授权恢复" }))).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to approve interrupt pause snapshot {}: {}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "授权失败" })),
            )
                .into_response()
        }
    }
}

#[utoipa::path(
    put,
    path = "/api/interrupt-pause/{id}/reject",
    params(
        ("id" = i64, Path, description = "Interrupt pause snapshot ID")
    ),
    request_body = RejectInterruptPauseRequest,
    responses(
        (status = 200, description = "Restore rejected")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn reject_interrupt_pause_snapshot(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
    Path(id): Path<i64>,
    Json(payload): Json<RejectInterruptPauseRequest>,
) -> impl IntoResponse {
    let reason = payload.reason.trim();
    if reason.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "请填写拒绝理由" })),
        )
            .into_response();
    }

    let result = sqlx::query(
        "UPDATE interrupt_pause_snapshots
         SET restore_status = 'rejected',
             reviewed_at = NOW(),
             reviewed_by = $1,
             reject_reason = $2,
             updated_at = NOW()
         WHERE id = $3",
    )
    .bind(&user.sub)
    .bind(reason)
    .bind(id)
    .execute(&state.db)
    .await;

    match result {
        Ok(done) if done.rows_affected() == 0 => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "记录不存在" })),
        )
            .into_response(),
        Ok(_) => {
            let _ = log_admin_action(
                &state.db,
                &user.sub,
                "reject_interrupt_pause",
                &format!("snapshot:{}", id),
                reason,
            )
            .await;

            (StatusCode::OK, Json(json!({ "message": "已拒绝恢复申请" }))).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to reject interrupt pause snapshot {}: {}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "拒绝失败" })),
            )
                .into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/plugin/interrupt-pause/save",
    request_body(content = PluginInterruptPauseSaveRequest, content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 200, description = "Snapshot saved"),
        (status = 401, description = "Invalid plugin token")
    )
)]
pub async fn save_interrupt_pause_snapshot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Form(payload): Form<PluginInterruptPauseSaveRequest>,
) -> impl IntoResponse {
    if let Err(response) = authorize_plugin(&headers) {
        return response;
    }

    if let Err(response) = ensure_server_exists(&state, payload.server_id).await {
        return response;
    }

    let auth_primary = payload.auth_primary.trim();
    let ip_address = payload.ip_address.trim();
    let map_name = payload.map_name.trim();
    let raw_payload = payload.payload.trim();

    if auth_primary.is_empty()
        || ip_address.is_empty()
        || map_name.is_empty()
        || raw_payload.is_empty()
    {
        return plugin_text_response(StatusCode::BAD_REQUEST, "invalid", "中断存档缺少必要字段");
    }

    let player_name = payload
        .player_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Unknown");

    let result = sqlx::query(
        "INSERT INTO interrupt_pause_snapshots (
            server_id, auth_primary, auth_steamid64, auth_steam3, auth_steam2, auth_engine,
            player_name, ip_address, map_name, mode, course, time_seconds,
            checkpoint_count, teleport_count, storage_version, payload,
            restore_status, restore_requested_at, reviewed_at, reviewed_by,
            reject_reason, restored_at, created_at, updated_at
         ) VALUES (
            $1, $2, NULLIF($3, ''), NULLIF($4, ''), NULLIF($5, ''), NULLIF($6, ''),
            $7, $8, $9, $10, $11, $12,
            $13, $14, $15, $16,
            'none', NULL, NULL, NULL,
            NULL, NULL, NOW(), NOW()
         )
         ON CONFLICT (server_id, auth_primary)
         DO UPDATE SET
            auth_steamid64 = EXCLUDED.auth_steamid64,
            auth_steam3 = EXCLUDED.auth_steam3,
            auth_steam2 = EXCLUDED.auth_steam2,
            auth_engine = EXCLUDED.auth_engine,
            player_name = EXCLUDED.player_name,
            ip_address = EXCLUDED.ip_address,
            map_name = EXCLUDED.map_name,
            mode = EXCLUDED.mode,
            course = EXCLUDED.course,
            time_seconds = EXCLUDED.time_seconds,
            checkpoint_count = EXCLUDED.checkpoint_count,
            teleport_count = EXCLUDED.teleport_count,
            storage_version = EXCLUDED.storage_version,
            payload = EXCLUDED.payload,
            restore_status = 'none',
            restore_requested_at = NULL,
            reviewed_at = NULL,
            reviewed_by = NULL,
            reject_reason = NULL,
            restored_at = NULL,
            updated_at = NOW()",
    )
    .bind(payload.server_id)
    .bind(auth_primary)
    .bind(normalize_optional(payload.auth_steamid64.as_deref()))
    .bind(normalize_optional(payload.auth_steam3.as_deref()))
    .bind(normalize_optional(payload.auth_steam2.as_deref()))
    .bind(normalize_optional(payload.auth_engine.as_deref()))
    .bind(player_name)
    .bind(ip_address)
    .bind(map_name)
    .bind(payload.mode)
    .bind(payload.course)
    .bind(payload.time_seconds)
    .bind(payload.checkpoint_count)
    .bind(payload.teleport_count)
    .bind(payload.storage_version)
    .bind(raw_payload)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => plugin_text_response(StatusCode::OK, "stored", "中断存档已保存"),
        Err(e) => {
            tracing::error!("Failed to save interrupt pause snapshot: {}", e);
            plugin_text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "error",
                "保存中断存档失败",
            )
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/plugin/interrupt-pause/peek",
    request_body(content = PluginInterruptPauseLookupRequest, content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 200, description = "Snapshot metadata")
    )
)]
pub async fn peek_interrupt_pause_snapshot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Form(payload): Form<PluginInterruptPauseLookupRequest>,
) -> impl IntoResponse {
    if let Err(response) = authorize_plugin(&headers) {
        return response;
    }

    if let Err(response) = ensure_server_exists(&state, payload.server_id).await {
        return response;
    }

    match lookup_active_snapshot(&state, &payload).await {
        Ok(Some(snapshot)) => plugin_snapshot_state_response(StatusCode::OK, &snapshot),
        Ok(None) => plugin_text_response(StatusCode::OK, "none", "没有可用的中断存档"),
        Err(e) => {
            tracing::error!("Failed to peek interrupt pause snapshot: {}", e);
            plugin_text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "error",
                "读取中断存档失败",
            )
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/plugin/interrupt-pause/request-restore",
    request_body(content = PluginInterruptPauseLookupRequest, content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 200, description = "Restore request submitted")
    )
)]
pub async fn request_interrupt_pause_restore(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Form(payload): Form<PluginInterruptPauseLookupRequest>,
) -> impl IntoResponse {
    if let Err(response) = authorize_plugin(&headers) {
        return response;
    }

    if let Err(response) = ensure_server_exists(&state, payload.server_id).await {
        return response;
    }

    let snapshot = match lookup_active_snapshot(&state, &payload).await {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => {
            return plugin_text_response(StatusCode::NOT_FOUND, "none", "没有找到中断存档");
        }
        Err(e) => {
            tracing::error!("Failed to lookup snapshot for restore request: {}", e);
            return plugin_text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "error",
                "读取中断存档失败",
            );
        }
    };

    match snapshot.restore_status.as_str() {
        "approved" => plugin_snapshot_state_response(StatusCode::OK, &snapshot),
        "pending" => plugin_snapshot_state_response(StatusCode::OK, &snapshot),
        _ => {
            let result = sqlx::query(
                "UPDATE interrupt_pause_snapshots
                 SET restore_status = 'pending',
                     restore_requested_at = NOW(),
                     reviewed_at = NULL,
                     reviewed_by = NULL,
                     reject_reason = NULL,
                     updated_at = NOW()
                 WHERE id = $1",
            )
            .bind(snapshot.id)
            .execute(&state.db)
            .await;

            match result {
                Ok(_) => plugin_text_response(
                    StatusCode::OK,
                    "pending",
                    "恢复申请已提交，请等待管理员审核",
                ),
                Err(e) => {
                    tracing::error!("Failed to request interrupt pause restore: {}", e);
                    plugin_text_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "error",
                        "提交恢复申请失败",
                    )
                }
            }
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/plugin/interrupt-pause/fetch-approved",
    request_body(content = PluginInterruptPauseLookupRequest, content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 200, description = "Approved snapshot payload")
    )
)]
pub async fn fetch_approved_interrupt_pause_snapshot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Form(payload): Form<PluginInterruptPauseLookupRequest>,
) -> impl IntoResponse {
    if let Err(response) = authorize_plugin(&headers) {
        return response;
    }

    if let Err(response) = ensure_server_exists(&state, payload.server_id).await {
        return response;
    }

    let snapshot = match lookup_active_snapshot(&state, &payload).await {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => {
            return plugin_text_response(StatusCode::NOT_FOUND, "none", "没有找到中断存档");
        }
        Err(e) => {
            tracing::error!("Failed to lookup approved interrupt pause snapshot: {}", e);
            return plugin_text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "error",
                "读取中断存档失败",
            );
        }
    };

    match snapshot.restore_status.as_str() {
        "approved" => payload_text_response(StatusCode::OK, &snapshot.payload),
        "rejected" => plugin_text_response(
            StatusCode::FORBIDDEN,
            "rejected",
            snapshot
                .reject_reason
                .as_deref()
                .unwrap_or("恢复申请已被拒绝"),
        ),
        "pending" => plugin_text_response(StatusCode::CONFLICT, "pending", "恢复申请仍在审核中"),
        _ => plugin_text_response(
            StatusCode::CONFLICT,
            "available",
            "请先提交恢复申请并等待管理员授权",
        ),
    }
}

#[utoipa::path(
    post,
    path = "/api/plugin/interrupt-pause/complete-restore",
    request_body(content = PluginInterruptPauseLookupRequest, content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 200, description = "Snapshot marked as restored")
    )
)]
pub async fn complete_interrupt_pause_restore(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Form(payload): Form<PluginInterruptPauseLookupRequest>,
) -> impl IntoResponse {
    if let Err(response) = authorize_plugin(&headers) {
        return response;
    }

    if let Err(response) = ensure_server_exists(&state, payload.server_id).await {
        return response;
    }

    let snapshot = match lookup_active_snapshot(&state, &payload).await {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => {
            return plugin_text_response(StatusCode::NOT_FOUND, "none", "没有找到中断存档");
        }
        Err(e) => {
            tracing::error!("Failed to lookup snapshot for restore completion: {}", e);
            return plugin_text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "error",
                "读取中断存档失败",
            );
        }
    };

    let result = sqlx::query(
        "UPDATE interrupt_pause_snapshots
         SET restore_status = 'restored',
             restored_at = NOW(),
             updated_at = NOW()
         WHERE id = $1",
    )
    .bind(snapshot.id)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => plugin_text_response(StatusCode::OK, "restored", "中断存档已恢复"),
        Err(e) => {
            tracing::error!("Failed to complete interrupt pause restore: {}", e);
            plugin_text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "error",
                "更新恢复状态失败",
            )
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/plugin/interrupt-pause/abort",
    request_body(content = PluginInterruptPauseLookupRequest, content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 200, description = "Snapshot aborted")
    )
)]
pub async fn abort_interrupt_pause_snapshot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Form(payload): Form<PluginInterruptPauseLookupRequest>,
) -> impl IntoResponse {
    if let Err(response) = authorize_plugin(&headers) {
        return response;
    }

    if let Err(response) = ensure_server_exists(&state, payload.server_id).await {
        return response;
    }

    let snapshot = match lookup_active_snapshot(&state, &payload).await {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => {
            return plugin_text_response(StatusCode::NOT_FOUND, "none", "没有找到中断存档");
        }
        Err(e) => {
            tracing::error!("Failed to lookup snapshot for abort: {}", e);
            return plugin_text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "error",
                "读取中断存档失败",
            );
        }
    };

    let result = sqlx::query(
        "UPDATE interrupt_pause_snapshots
         SET restore_status = 'aborted',
             updated_at = NOW()
         WHERE id = $1",
    )
    .bind(snapshot.id)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => plugin_text_response(StatusCode::OK, "aborted", "已终止中断存档"),
        Err(e) => {
            tracing::error!("Failed to abort interrupt pause snapshot: {}", e);
            plugin_text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "error",
                "终止中断存档失败",
            )
        }
    }
}

fn authorize_plugin(headers: &HeaderMap) -> Result<(), Response> {
    let expected = match env::var("PLUGIN_API_TOKEN") {
        Ok(token) if !token.trim().is_empty() => token,
        _ => {
            tracing::error!("PLUGIN_API_TOKEN is not configured");
            return Err(plugin_text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "error",
                "插件鉴权令牌未配置",
            ));
        }
    };

    let provided = headers
        .get(PLUGIN_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    if provided == expected {
        Ok(())
    } else {
        Err(plugin_text_response(
            StatusCode::UNAUTHORIZED,
            "error",
            "无效的插件令牌",
        ))
    }
}

async fn ensure_server_exists(state: &Arc<AppState>, server_id: i64) -> Result<(), Response> {
    let exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM servers WHERE id = $1)")
            .bind(server_id)
            .fetch_one(&state.db)
            .await;

    match exists {
        Ok(true) => Ok(()),
        Ok(false) => Err(plugin_text_response(
            StatusCode::NOT_FOUND,
            "error",
            "服务器未注册",
        )),
        Err(e) => {
            tracing::error!(
                "Failed to validate interrupt pause server {}: {}",
                server_id,
                e
            );
            Err(plugin_text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "error",
                "验证服务器信息失败",
            ))
        }
    }
}

async fn lookup_active_snapshot(
    state: &Arc<AppState>,
    payload: &PluginInterruptPauseLookupRequest,
) -> Result<Option<PluginInterruptPauseSnapshotRow>, sqlx::Error> {
    let auth_primary = normalize_optional(payload.auth_primary.as_deref());
    let auth_steamid64 = normalize_optional(payload.auth_steamid64.as_deref());
    let auth_steam3 = normalize_optional(payload.auth_steam3.as_deref());
    let auth_steam2 = normalize_optional(payload.auth_steam2.as_deref());
    let auth_engine = normalize_optional(payload.auth_engine.as_deref());

    if auth_primary.is_empty()
        && auth_steamid64.is_empty()
        && auth_steam3.is_empty()
        && auth_steam2.is_empty()
        && auth_engine.is_empty()
    {
        return Ok(None);
    }

    sqlx::query_as::<_, PluginInterruptPauseSnapshotRow>(
        "SELECT
            id,
            server_id,
            auth_primary,
            auth_steamid64,
            auth_steam3,
            auth_steam2,
            auth_engine,
            player_name,
            ip_address,
            map_name,
            mode,
            course,
            time_seconds,
            checkpoint_count,
            teleport_count,
            storage_version,
            payload,
            restore_status,
            restore_requested_at,
            reviewed_at,
            reviewed_by,
            reject_reason,
            restored_at,
            created_at,
            updated_at
         FROM interrupt_pause_snapshots
         WHERE server_id = $1
           AND restore_status NOT IN ('aborted', 'restored')
           AND (
                ($2 <> '' AND (auth_primary = $2 OR auth_steamid64 = $2 OR auth_steam3 = $2 OR auth_steam2 = $2 OR auth_engine = $2))
             OR ($3 <> '' AND (auth_primary = $3 OR auth_steamid64 = $3 OR auth_steam3 = $3 OR auth_steam2 = $3 OR auth_engine = $3))
             OR ($4 <> '' AND (auth_primary = $4 OR auth_steamid64 = $4 OR auth_steam3 = $4 OR auth_steam2 = $4 OR auth_engine = $4))
             OR ($5 <> '' AND (auth_primary = $5 OR auth_steamid64 = $5 OR auth_steam3 = $5 OR auth_steam2 = $5 OR auth_engine = $5))
             OR ($6 <> '' AND (auth_primary = $6 OR auth_steamid64 = $6 OR auth_steam3 = $6 OR auth_steam2 = $6 OR auth_engine = $6))
           )
         ORDER BY updated_at DESC
         LIMIT 1",
    )
    .bind(payload.server_id)
    .bind(auth_primary)
    .bind(auth_steamid64)
    .bind(auth_steam3)
    .bind(auth_steam2)
    .bind(auth_engine)
    .fetch_optional(&state.db)
    .await
}

fn normalize_optional(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("")
        .to_string()
}

fn plugin_snapshot_state_response(
    status_code: StatusCode,
    snapshot: &PluginInterruptPauseSnapshotRow,
) -> Response {
    let plugin_status = match snapshot.restore_status.as_str() {
        "none" => "available",
        other => other,
    };

    let message = match snapshot.restore_status.as_str() {
        "approved" => "已获得管理员授权，可恢复中断存档",
        "pending" => "恢复申请审核中",
        "rejected" => snapshot
            .reject_reason
            .as_deref()
            .unwrap_or("恢复申请已被拒绝"),
        _ => "存在中断存档，可发起恢复申请",
    };

    let reject_reason = snapshot.reject_reason.as_deref().unwrap_or("");
    let body = format!(
        "status={}\nmessage={}\nid={}\nmap_name={}\ntime_seconds={:.3}\ncheckpoint_count={}\nteleport_count={}\nmode={}\ncourse={}\nreject_reason={}\n",
        plugin_status,
        sanitize_text(message),
        snapshot.id,
        sanitize_text(&snapshot.map_name),
        snapshot.time_seconds,
        snapshot.checkpoint_count,
        snapshot.teleport_count,
        snapshot.mode,
        snapshot.course,
        sanitize_text(reject_reason),
    );

    text_response(status_code, &body)
}

fn plugin_text_response(status: StatusCode, plugin_status: &str, message: &str) -> Response {
    let body = format!(
        "status={}\nmessage={}\n",
        plugin_status,
        sanitize_text(message),
    );
    text_response(status, &body)
}

fn payload_text_response(status: StatusCode, body: &str) -> Response {
    text_response(status, body)
}

fn text_response(status: StatusCode, body: &str) -> Response {
    let mut response = (status, body.to_string()).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

fn sanitize_text(value: &str) -> String {
    value.replace('\n', " ").replace('\r', " ")
}
