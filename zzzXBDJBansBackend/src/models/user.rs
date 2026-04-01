use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use chrono::{DateTime, Utc};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Admin {
    pub id: i64,
    pub username: String,
    #[serde(skip_serializing)] 
    pub password: String,
    pub role: String, // Enum in DB, String here for simplicity or use sqlx::Type
    pub steam_id: Option<String>,
    pub steam_id_3: Option<String>,
    pub steam_id_64: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub remark: Option<String>,
}


#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateAdminRequest {
    pub username: String,
    pub password: String,
    pub role: String,
    pub steam_id: Option<String>,
    pub remark: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateAdminRequest {
    pub username: Option<String>,
    pub password: Option<String>,
    pub role: Option<String>,
    pub steam_id: Option<String>,
    pub remark: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LoginResponse {
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    pub old_password: String,
    pub new_password: String,
}
