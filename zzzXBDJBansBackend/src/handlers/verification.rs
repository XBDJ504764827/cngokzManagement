use crate::handlers::auth::Claims;
use crate::AppState;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct VerificationRecord {
    pub steam_id: String,
    pub status: String,
    pub reason: Option<String>,
    pub steam_level: Option<i32>,
    pub playtime_minutes: Option<i32>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateVerificationRequest {
    pub steam_id: String,
    pub status: Option<String>, // 'pending', 'allowed', 'denied'
    pub reason: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateVerificationRequest {
    pub status: Option<String>,
    pub reason: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/verifications",
    responses(
        (status = 200, description = "List verification records", body = Vec<VerificationRecord>)
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn list_verifications(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
) -> impl IntoResponse {
    if claims.role != "super_admin" {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Access denied" })),
        )
            .into_response();
    }

    let rows = match sqlx::query("SELECT steam_id, status, reason, steam_level, playtime_minutes, created_at, updated_at FROM player_verifications ORDER BY created_at DESC")
        .fetch_all(&state.db)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };

    let records = rows
        .into_iter()
        .map(|row| VerificationRecord {
            steam_id: row.get("steam_id"),
            status: row.get("status"),
            reason: row.get("reason"),
            steam_level: row.get("steam_level"),
            playtime_minutes: row.get("playtime_minutes"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
        .collect::<Vec<_>>();

    (StatusCode::OK, Json(records)).into_response()
}

#[utoipa::path(
    post,
    path = "/api/verifications",
    request_body = CreateVerificationRequest,
    responses(
        (status = 200, description = "Record created", body = VerificationRecord),
        (status = 500, description = "Already exists or error")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn create_verification(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<CreateVerificationRequest>,
) -> impl IntoResponse {
    if claims.role != "super_admin" {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Access denied" })),
        )
            .into_response();
    }

    let status = payload.status.unwrap_or_else(|| "pending".to_string());

    if !["pending", "verified", "allowed"].contains(&status.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": format!(
                    "Invalid status '{}'. Allowed: pending, verified, allowed",
                    status
                )
            })),
        )
            .into_response();
    }

    let exists: bool =
        sqlx::query_scalar("SELECT COUNT(*) FROM player_verifications WHERE steam_id = $1")
            .bind(&payload.steam_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
            > 0;

    if exists {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "Verification record already exists for this SteamID" })),
        )
            .into_response();
    }

    if let Err(e) = sqlx::query(
        "INSERT INTO player_verifications (steam_id, status, reason) VALUES ($1, $2, $3)",
    )
    .bind(&payload.steam_id)
    .bind(&status)
    .bind(&payload.reason)
    .execute(&state.db)
    .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    let row = match sqlx::query("SELECT steam_id, status, reason, steam_level, playtime_minutes, created_at, updated_at FROM player_verifications WHERE steam_id = $1")
        .bind(&payload.steam_id)
        .fetch_one(&state.db)
        .await
    {
        Ok(row) => row,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };

    (
        StatusCode::OK,
        Json(VerificationRecord {
            steam_id: row.get("steam_id"),
            status: row.get("status"),
            reason: row.get("reason"),
            steam_level: row.get("steam_level"),
            playtime_minutes: row.get("playtime_minutes"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }),
    )
        .into_response()
}

#[utoipa::path(
    put,
    path = "/api/verifications/{steam_id}",
    params(
        ("steam_id" = String, Path, description = "Steam ID")
    ),
    request_body = UpdateVerificationRequest,
    responses(
        (status = 200, description = "Record updated", body = VerificationRecord)
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn update_verification(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
    Path(steam_id): Path<String>,
    Json(payload): Json<UpdateVerificationRequest>,
) -> impl IntoResponse {
    if claims.role != "super_admin" {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Access denied" })),
        )
            .into_response();
    }

    if let Some(s) = &payload.status {
        if !["pending", "verified", "allowed"].contains(&s.as_str()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!(
                        "Invalid status '{}'. Allowed: pending, verified, allowed",
                        s
                    )
                })),
            )
                .into_response();
        }
        if let Err(e) =
            sqlx::query("UPDATE player_verifications SET status = $1 WHERE steam_id = $2")
                .bind(s)
                .bind(&steam_id)
                .execute(&state.db)
                .await
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    }

    if let Some(r) = &payload.reason {
        if let Err(e) =
            sqlx::query("UPDATE player_verifications SET reason = $1 WHERE steam_id = $2")
                .bind(r)
                .bind(&steam_id)
                .execute(&state.db)
                .await
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    }

    let row = match sqlx::query("SELECT steam_id, status, reason, steam_level, playtime_minutes, created_at, updated_at FROM player_verifications WHERE steam_id = $1")
        .bind(&steam_id)
        .fetch_one(&state.db)
        .await
    {
        Ok(row) => row,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };

    (
        StatusCode::OK,
        Json(VerificationRecord {
            steam_id: row.get("steam_id"),
            status: row.get("status"),
            reason: row.get("reason"),
            steam_level: row.get("steam_level"),
            playtime_minutes: row.get("playtime_minutes"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }),
    )
        .into_response()
}

#[utoipa::path(
    delete,
    path = "/api/verifications/{steam_id}",
    params(
        ("steam_id" = String, Path, description = "Steam ID")
    ),
    responses(
        (status = 204, description = "Record deleted")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn delete_verification(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
    Path(steam_id): Path<String>,
) -> impl IntoResponse {
    if claims.role != "super_admin" {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Access denied" })),
        )
            .into_response();
    }

    match sqlx::query("DELETE FROM player_verifications WHERE steam_id = $1")
        .bind(steam_id)
        .execute(&state.db)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
