use crate::handlers::auth::Claims;
use crate::models::log::{AuditLog, CreateLogRequest};
use crate::AppState;
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;

#[utoipa::path(
    get,
    path = "/api/logs",
    responses(
        (status = 200, description = "List logs", body = Vec<AuditLog>),
        (status = 403, description = "Forbidden")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn list_logs(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
) -> impl IntoResponse {
    if claims.role != "super_admin" {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }

    let logs = sqlx::query_as::<_, AuditLog>(
        "SELECT * FROM audit_logs ORDER BY created_at DESC LIMIT 100",
    )
    .fetch_all(&state.db)
    .await;

    match logs {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/logs",
    request_body = CreateLogRequest,
    responses(
        (status = 201, description = "Log created")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn create_log(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<CreateLogRequest>,
) -> impl IntoResponse {
    let result = sqlx::query(
        "INSERT INTO audit_logs (admin_username, action, target, details) VALUES ($1, $2, $3, $4)",
    )
    .bind(claims.sub)
    .bind(payload.action)
    .bind(payload.target)
    .bind(payload.details)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => (StatusCode::CREATED, Json("Log created")).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
