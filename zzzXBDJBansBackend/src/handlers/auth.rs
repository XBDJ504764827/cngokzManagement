use crate::models::user::{AuthUser, ChangePasswordRequest, LoginRequest, LoginResponse};
use crate::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use bcrypt::verify;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    #[serde(default)]
    pub id: Option<i64>,
    pub sub: String, // username
    pub role: String,
    pub exp: usize,
}

fn jwt_secret() -> Result<String, StatusCode> {
    std::env::var("JWT_SECRET").map_err(|_| {
        tracing::error!("JWT_SECRET is not set");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[utoipa::path(
    post,
    path = "/api/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    let row =
        sqlx::query_as::<_, crate::models::user::Admin>("SELECT * FROM admins WHERE username = $1")
            .bind(&payload.username)
            .fetch_optional(&state.db)
            .await;

    match row {
        Ok(Some(user)) => {
            // Verify password
            // Note: In a real app we use bcrypt.
            // For now, if string matches (for initial plaintext) OR bcrypt verify.
            // Our init migration inserts a bcrypt hash '$2y$10$...'
            // We should use bcrypt::verify.

            let valid = verify(&payload.password, &user.password).unwrap_or(false);

            if valid {
                tracing::info!("Login successful for user: {}", user.username);
                // Generate JWT
                let expiration = chrono::Utc::now()
                    .checked_add_signed(chrono::Duration::days(1))
                    .expect("valid timestamp")
                    .timestamp();

                let claims = Claims {
                    id: Some(user.id),
                    sub: user.username.clone(),
                    role: user.role.clone(),
                    exp: expiration as usize,
                };

                let secret = match jwt_secret() {
                    Ok(secret) => secret,
                    Err(status) => return status.into_response(),
                };

                let token = match encode(
                    &Header::default(),
                    &claims,
                    &EncodingKey::from_secret(secret.as_ref()),
                ) {
                    Ok(token) => token,
                    Err(e) => {
                        tracing::error!("Failed to encode JWT for user '{}': {}", user.username, e);
                        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                    }
                };

                let response = LoginResponse {
                    token,
                    user: AuthUser {
                        id: user.id,
                        username: user.username,
                        role: user.role,
                    },
                };

                return (StatusCode::OK, Json(response)).into_response();
            } else {
                tracing::warn!(
                    "Login failed for user: {} (Invalid password)",
                    payload.username
                );
            }
        }
        Ok(None) => {
            tracing::warn!("Login failed: User '{}' not found", payload.username);
        }
        Err(e) => {
            tracing::error!(
                "Database error during login for user '{}': {}",
                payload.username,
                e
            );
        }
    }

    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "Invalid credentials" })),
    )
        .into_response()
}

#[utoipa::path(
    post,
    path = "/api/auth/logout",
    responses(
        (status = 200, description = "Logged out")
    )
)]
pub async fn logout() -> impl IntoResponse {
    // Stateless JWT, client just drops token.
    // We can blacklist token in Redis if stricter.
    (StatusCode::OK, Json(json!({ "msg": "Logged out" })))
}

#[utoipa::path(
    get,
    path = "/api/auth/me",
    responses(
        (status = 200, description = "Current user info", body = AuthUser)
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn me(
    axum::extract::Extension(user): axum::extract::Extension<Claims>,
) -> impl IntoResponse {
    let Some(id) = user.id else {
        tracing::warn!("JWT claims missing admin id for user '{}'", user.sub);
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid credentials" })),
        )
            .into_response();
    };

    let response = AuthUser {
        id,
        username: user.sub,
        role: user.role,
    };

    (StatusCode::OK, Json(response)).into_response()
}

use bcrypt::{hash, DEFAULT_COST};

#[utoipa::path(
    post,
    path = "/api/auth/change-password",
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "Password changed successfully"),
        (status = 400, description = "Invalid old password"),
        (status = 401, description = "Unauthorized")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn change_password(
    State(state): State<Arc<AppState>>,
    axum::extract::Extension(user): axum::extract::Extension<Claims>,
    Json(payload): Json<crate::models::user::ChangePasswordRequest>,
) -> impl IntoResponse {
    // 1. Fetch current user
    let row =
        sqlx::query_as::<_, crate::models::user::Admin>("SELECT * FROM admins WHERE username = $1")
            .bind(&user.sub)
            .fetch_optional(&state.db)
            .await;

    match row {
        Ok(Some(admin)) => {
            // 2. Verify Old Password
            let valid = verify(&payload.old_password, &admin.password).unwrap_or(false);
            if !valid {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": "Old password incorrect" })),
                )
                    .into_response();
            }

            // 3. Update to New Password
            let hashed = hash(payload.new_password, DEFAULT_COST).unwrap();
            let update = sqlx::query("UPDATE admins SET password = $1 WHERE id = $2")
                .bind(hashed)
                .bind(admin.id)
                .execute(&state.db)
                .await;

            match update {
                Ok(_) => {
                    // Log functionality (optional)
                    let _ = crate::utils::log_admin_action(
                        &state.db,
                        &user.sub,
                        "change_password",
                        "Self",
                        "Changed own password",
                    )
                    .await;

                    (
                        StatusCode::OK,
                        Json(json!({ "message": "Password updated successfully" })),
                    )
                        .into_response()
                }
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "User not found" })),
        )
            .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
