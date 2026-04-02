use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use crate::AppState;
use crate::models::user::{Admin, CreateAdminRequest, UpdateAdminRequest};
use crate::handlers::auth::Claims;
use crate::utils::log_admin_action;
use bcrypt::{hash, DEFAULT_COST};

fn is_super_admin(user: &Claims) -> bool {
    user.role == "super_admin"
}

fn is_valid_admin_role(role: &str) -> bool {
    matches!(role, "admin" | "super_admin")
}

async fn resolve_actor_admin_id(
    state: &Arc<AppState>,
    user: &Claims,
) -> Result<i64, axum::response::Response> {
    if let Some(id) = user.id {
        return Ok(id);
    }

    match sqlx::query_scalar::<_, i64>("SELECT id FROM admins WHERE username = $1")
        .bind(&user.sub)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(id)) => Ok(id),
        Ok(None) => Err((StatusCode::UNAUTHORIZED, Json("Admin not found")).into_response()),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(e.to_string())).into_response()),
    }
}

#[utoipa::path(
    get,
    path = "/api/admins",
    responses(
        (status = 200, description = "List all admins", body = Vec<Admin>)
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn list_admins(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
) -> impl IntoResponse {
    let admins = if is_super_admin(&user) {
        sqlx::query_as::<_, Admin>("SELECT * FROM admins ORDER BY created_at DESC, id DESC")
            .fetch_all(&state.db)
            .await
    } else {
        let actor_id = match resolve_actor_admin_id(&state, &user).await {
            Ok(id) => id,
            Err(response) => return response,
        };

        sqlx::query_as::<_, Admin>("SELECT * FROM admins WHERE id = $1")
            .bind(actor_id)
            .fetch_all(&state.db)
            .await
    };

    match admins {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/admins",
    request_body = CreateAdminRequest,
    responses(
        (status = 201, description = "Admin created"),
        (status = 400, description = "Bad request")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn create_admin(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
    Json(payload): Json<CreateAdminRequest>,
) -> impl IntoResponse {
    if !is_super_admin(&user) {
        return (StatusCode::FORBIDDEN, Json("Only super admins can create admins")).into_response();
    }

    if !is_valid_admin_role(&payload.role) {
        return (StatusCode::BAD_REQUEST, Json("Invalid admin role")).into_response();
    }

    let hashed = match hash(payload.password, DEFAULT_COST) {
        Ok(hashed) => hashed,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(e.to_string())).into_response(),
    };

    // 解析 SteamID 为各种格式
    let (steam_id_2, steam_id_3, steam_id_64) = if let Some(ref input_steam_id) = payload.steam_id {
        let steam_service = state.steam_service.as_ref();
        let id64 = steam_service.resolve_steam_id(input_steam_id).await
            .unwrap_or_else(|| input_steam_id.clone());
        
        let id2 = steam_service.id64_to_id2(&id64);
        let id3 = steam_service.id64_to_id3(&id64);
        
        (id2, id3, Some(id64))
    } else {
        (None, None, None)
    };

    let result = sqlx::query(
        "INSERT INTO admins (username, password, role, steam_id, steam_id_3, steam_id_64, remark) VALUES ($1, $2, $3, $4, $5, $6, $7)"
    )
    .bind(&payload.username)
    .bind(hashed)
    .bind(&payload.role)
    .bind(&steam_id_2)
    .bind(&steam_id_3)
    .bind(&steam_id_64)
    .bind(&payload.remark)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => {
            let _ = log_admin_action(
                &state.db,
                &user.sub,
                "create_admin",
                &payload.username,
                &format!("Role: {}", payload.role)
            ).await;
            (StatusCode::CREATED, Json("Admin created")).into_response()
        },
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    put,
    path = "/api/admins/{id}",
    params(
        ("id" = i64, Path, description = "Admin ID")
    ),
    request_body = UpdateAdminRequest,
    responses(
        (status = 200, description = "Admin updated"),
        (status = 404, description = "Admin not found")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn update_admin(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateAdminRequest>,
) -> impl IntoResponse {
    let actor_id = match resolve_actor_admin_id(&state, &user).await {
        Ok(id) => id,
        Err(response) => return response,
    };
    let actor_is_super_admin = is_super_admin(&user);

    if !actor_is_super_admin && actor_id != id {
        return (StatusCode::FORBIDDEN, Json("You can only update your own account")).into_response();
    }

    let existing = match sqlx::query_as::<_, Admin>("SELECT * FROM admins WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(admin)) => admin,
        Ok(None) => return (StatusCode::NOT_FOUND, Json("Admin not found")).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(e.to_string())).into_response(),
    };

    if !actor_is_super_admin {
        if let Some(role) = &payload.role {
            if role != &existing.role {
                return (StatusCode::FORBIDDEN, Json("Only super admins can change roles")).into_response();
            }
        }
    } else if let Some(role) = &payload.role {
        if !is_valid_admin_role(role) {
            return (StatusCode::BAD_REQUEST, Json("Invalid admin role")).into_response();
        }
    }

    let username = payload.username.unwrap_or(existing.username.clone());
    let role = if actor_is_super_admin {
        payload.role.unwrap_or(existing.role.clone())
    } else {
        existing.role.clone()
    };
    let password = match payload.password {
        Some(password) => match hash(password, DEFAULT_COST) {
            Ok(hashed) => hashed,
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(e.to_string())).into_response(),
        },
        None => existing.password.clone(),
    };
    let remark = payload.remark.or(existing.remark.clone());

    let (steam_id, steam_id_3, steam_id_64) = match payload.steam_id {
        Some(input_steam_id) => {
            let trimmed = input_steam_id.trim().to_string();
            if trimmed.is_empty() {
                (None, None, None)
            } else {
                let steam_service = state.steam_service.as_ref();
                let id64 = steam_service
                    .resolve_steam_id(&trimmed)
                    .await
                    .unwrap_or(trimmed.clone());
                let id2 = steam_service.id64_to_id2(&id64);
                let id3 = steam_service.id64_to_id3(&id64);
                (id2, id3, Some(id64))
            }
        }
        None => (
            existing.steam_id.clone(),
            existing.steam_id_3.clone(),
            existing.steam_id_64.clone(),
        ),
    };

    let result = sqlx::query(
        "UPDATE admins
         SET username = $1, password = $2, role = $3, steam_id = $4, steam_id_3 = $5, steam_id_64 = $6, remark = $7
         WHERE id = $8"
    )
    .bind(&username)
    .bind(&password)
    .bind(&role)
    .bind(&steam_id)
    .bind(&steam_id_3)
    .bind(&steam_id_64)
    .bind(&remark)
    .bind(id)
    .execute(&state.db)
    .await;

    if let Err(e) = result {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(e.to_string())).into_response();
    }

    let _ = log_admin_action(
        &state.db,
        &user.sub,
        "update_admin",
        &format!("AdminID: {}", id),
        "Updated admin details"
    ).await;

    (StatusCode::OK, Json("Admin updated")).into_response()
}

#[utoipa::path(
    delete,
    path = "/api/admins/{id}",
    params(
        ("id" = i64, Path, description = "Admin ID")
    ),
    responses(
        (status = 200, description = "Admin deleted"),
        (status = 404, description = "Admin not found")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn delete_admin(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<Claims>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    if !is_super_admin(&user) {
        return (StatusCode::FORBIDDEN, Json("Only super admins can delete admins")).into_response();
    }

    let actor_id = match resolve_actor_admin_id(&state, &user).await {
        Ok(id) => id,
        Err(response) => return response,
    };

    if actor_id == id {
        return (StatusCode::BAD_REQUEST, Json("You cannot delete your own account")).into_response();
    }

    let result = sqlx::query("DELETE FROM admins WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await;

    match result {
        Ok(_) => {
             let _ = log_admin_action(
                &state.db,
                &user.sub,
                "delete_admin",
                &format!("AdminID: {}", id),
                "Deleted admin"
            ).await;
            (StatusCode::OK, Json("Admin deleted")).into_response()
        },
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
