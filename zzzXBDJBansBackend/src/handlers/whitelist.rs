use axum::{
    extract::{Path, State, Extension},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use crate::handlers::auth::Claims;
use std::sync::Arc;
use crate::{AppState, models::whitelist::{Whitelist, CreateWhitelistRequest, ApplyWhitelistRequest, RejectWhitelistRequest}};
use serde_json::json;
use crate::services::steam_api::{SteamService, PlayerSummary};

// 获取已审核通过的白名单列表（管理员）
#[utoipa::path(
    get,
    path = "/api/whitelist",
    responses(
        (status = 200, description = "List approved whitelist", body = Vec<Whitelist>)
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn list_whitelist(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let whitelist = sqlx::query_as::<_, Whitelist>("SELECT * FROM whitelist WHERE status = 'approved' ORDER BY created_at DESC")
        .fetch_all(&state.db)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Failed to fetch whitelist: {:?}", e);
            vec![]
        });

    Json(whitelist)
}

// 获取待审核的申请列表（管理员）
#[utoipa::path(
    get,
    path = "/api/whitelist/pending",
    responses(
        (status = 200, description = "List pending applications", body = Vec<Whitelist>)
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn list_pending(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let pending = sqlx::query_as::<_, Whitelist>("SELECT * FROM whitelist WHERE status = 'pending' ORDER BY created_at DESC")
        .fetch_all(&state.db)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Failed to fetch pending whitelist: {:?}", e);
            vec![]
        });

    Json(pending)
}

// 获取已拒绝的申请列表（管理员）
#[utoipa::path(
    get,
    path = "/api/whitelist/rejected",
    responses(
        (status = 200, description = "List rejected applications", body = Vec<Whitelist>)
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn list_rejected(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let rejected = sqlx::query_as::<_, Whitelist>("SELECT * FROM whitelist WHERE status = 'rejected' ORDER BY created_at DESC")
        .fetch_all(&state.db)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Failed to fetch rejected whitelist: {:?}", e);
            vec![]
        });

    Json(rejected)
}

// 玩家提交申请（公开接口，无需认证）
#[utoipa::path(
    post,
    path = "/api/whitelist/apply",
    request_body = ApplyWhitelistRequest,
    responses(
        (status = 201, description = "Application submitted"),
        (status = 400, description = "Invalid format"),
        (status = 409, description = "Already exists")
    )
)]
pub async fn apply_whitelist(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ApplyWhitelistRequest>,
) -> impl IntoResponse {
    let steam_service = SteamService::new();
    
    // 解析输入的 SteamID 为各种格式
    // 严格模式：resolve_steam_id 如果返回 Some，表示解析成功。
    // 我们必须确保能拿到 ID64, ID3, ID2
    let steam_id_64_opt = steam_service.resolve_steam_id(&payload.steam_id).await;
    
    if steam_id_64_opt.is_none() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "SteamID 格式无效，请检查" })));
    }
    
    let steam_id_64 = steam_id_64_opt.unwrap();
    let steam_id_2_opt = steam_service.id64_to_id2(&steam_id_64);
    let steam_id_3 = steam_service.id64_to_id3(&steam_id_64); 

    // 确保三种格式都存在
    if steam_id_2_opt.is_none() || steam_id_3.is_none() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "无法解析 SteamID 格式" })));
    }
    let steam_id_2 = steam_id_2_opt.unwrap();

    // 检查是否已存在（任何状态）
    // 获取已存在的记录状态
    let existing_status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM whitelist WHERE steam_id_64 = ? OR steam_id = ?"
    )
    .bind(&steam_id_64)
    .bind(&steam_id_2)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    if let Some(status) = existing_status {
        let msg = match status.as_str() {
            "approved" => "您已在白名单中",
            "pending" => "您已经提交请等待管理员审核",
            "rejected" => "您未通过白名单审核，如有异议请联系群管理员",
            _ => "您已存在记录",
        };
        return (StatusCode::CONFLICT, Json(json!({ "error": msg, "status": status })));
    }

    let result = sqlx::query(
        "INSERT INTO whitelist (steam_id, steam_id_3, steam_id_64, name, status) VALUES (?, ?, ?, ?, 'pending')",
    )
    .bind(&steam_id_2)
    .bind(&steam_id_3)
    .bind(&steam_id_64)
    .bind(&payload.name)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => (StatusCode::CREATED, Json(json!({ "message": "申请已提交，请等待管理员审核" }))),
        Err(e) => {
            tracing::error!("Failed to submit whitelist application: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "提交申请失败" })))
        }
    }
}

// 管理员添加白名单（直接通过）
#[utoipa::path(
    post,
    path = "/api/whitelist",
    request_body = CreateWhitelistRequest,
    responses(
        (status = 201, description = "Whitelist added manually"),
        (status = 400, description = "Bad request")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn create_whitelist(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
    Json(payload): Json<CreateWhitelistRequest>,
) -> impl IntoResponse {
    let steam_service = SteamService::new();
    
    // 解析输入的 SteamID 为各种格式
    let steam_id_64_opt = steam_service.resolve_steam_id(&payload.steam_id).await;
    
    if steam_id_64_opt.is_none() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid SteamID format" })));
    }
    
    let steam_id_64 = steam_id_64_opt.unwrap();
    let steam_id_2_opt = steam_service.id64_to_id2(&steam_id_64);
    let steam_id_3 = steam_service.id64_to_id3(&steam_id_64);

    if steam_id_2_opt.is_none() || steam_id_3.is_none() {
         return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Cannot resolve SteamID variants" })));
    }
    let steam_id_2 = steam_id_2_opt.unwrap();


    let result = sqlx::query(
        "INSERT INTO whitelist (steam_id, steam_id_3, steam_id_64, name, status, admin_name) VALUES (?, ?, ?, ?, 'approved', ?)",
    )
    .bind(&steam_id_2)
    .bind(&steam_id_3)
    .bind(&steam_id_64)
    .bind(&payload.name)
    .bind(&user.sub)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => (StatusCode::CREATED, Json(json!({ "message": "Whitelist added" }))),
        Err(e) => {
            tracing::error!("Failed to add whitelist: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Failed to add whitelist or duplicate entry" })))
        }
    }
}

// 审核通过
#[utoipa::path(
    put,
    path = "/api/whitelist/{id}/approve",
    params(
        ("id" = i64, Path, description = "Whitelist ID")
    ),
    responses(
        (status = 200, description = "Application approved")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn approve_whitelist(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let result = sqlx::query("UPDATE whitelist SET status = 'approved', admin_name = ? WHERE id = ?")
        .bind(&user.sub)
        .bind(id)
        .execute(&state.db)
        .await;

    match result {
        Ok(_) => (StatusCode::OK, Json(json!({ "message": "已审核通过" }))),
        Err(e) => {
            tracing::error!("Failed to approve whitelist: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "审核失败" })))
        }
    }
}

// 审核拒绝
#[utoipa::path(
    put,
    path = "/api/whitelist/{id}/reject",
    params(
        ("id" = i64, Path, description = "Whitelist ID")
    ),
    request_body = RejectWhitelistRequest,
    responses(
        (status = 200, description = "Application rejected")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn reject_whitelist(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
    Path(id): Path<i64>,
    Json(payload): Json<RejectWhitelistRequest>,
) -> impl IntoResponse {
    let result = sqlx::query("UPDATE whitelist SET status = 'rejected', reject_reason = ?, admin_name = ? WHERE id = ?")
        .bind(&payload.reason)
        .bind(&user.sub)
        .bind(id)
        .execute(&state.db)
        .await;

    match result {
        Ok(_) => (StatusCode::OK, Json(json!({ "message": "已拒绝" }))),
        Err(e) => {
            tracing::error!("Failed to reject whitelist: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "拒绝失败" })))
        }
    }
}

// 删除白名单
#[utoipa::path(
    delete,
    path = "/api/whitelist/{id}",
    params(
        ("id" = i64, Path, description = "Whitelist ID")
    ),
    responses(
        (status = 200, description = "Entry deleted")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn delete_whitelist(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let result = sqlx::query("DELETE FROM whitelist WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await;

    match result {
        Ok(_) => (StatusCode::OK, Json(json!({ "message": "Whitelist deleted" }))),
        Err(e) => {
            tracing::error!("Failed to delete whitelist: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Failed to delete whitelist" })))
        }
    }
}

// 公开接口：获取所有白名单状态
#[utoipa::path(
    get,
    path = "/api/whitelist/public-list",
    responses(
        (status = 200, description = "Public whitelist check", body = Vec<Whitelist>)
    )
)]
pub async fn list_public_whitelist(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    // 查询所有记录，按时间倒序
    // 注意：这里返回了完整结构体，包含 SteamID。如果不想公开 SteamID，需要定义一个新的结构体只包含 Name, Status, Time
    // 根据用户需求"展示白名单通过的玩家，正在审核的玩家，被拒绝的玩家"，通常需要 ID 来确认是自己
    let list = sqlx::query_as::<_, Whitelist>("SELECT * FROM whitelist ORDER BY created_at DESC")
        .fetch_all(&state.db)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Failed to fetch public whitelist: {:?}", e);
            vec![]
        });

    Json(list)
}

// 获取玩家 Steam 信息（公开接口）
#[utoipa::path(
    get,
    path = "/api/whitelist/player-info",
    params(
        ("steam_id" = String, Query, description = "Steam ID (any format)")
    ),
    responses(
        (status = 200, description = "Player info found", body = PlayerSummary),
        (status = 404, description = "Player not found"),
        (status = 400, description = "Invalid Steam ID")
    )
)]
pub async fn get_player_info(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let steam_id = params.get("steam_id");
    if steam_id.is_none() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing steam_id" }))).into_response();
    }
    let steam_id = steam_id.unwrap();

    let steam_service = SteamService::new();
    let steam_id_64_opt = steam_service.resolve_steam_id(steam_id).await;

    if let Some(steam_id_64) = steam_id_64_opt {
        if let Some(summary) = steam_service.get_player_summary(&steam_id_64).await {
             return (StatusCode::OK, Json(summary)).into_response();
        }
    }

    (StatusCode::NOT_FOUND, Json(json!({ "error": "Player not found" }))).into_response()
}
