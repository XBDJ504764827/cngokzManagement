use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone, ToSchema)]
pub struct Ban {
    pub id: i64,
    pub name: String,
    pub steam_id: String,
    pub steam_id_3: Option<String>,
    pub steam_id_64: Option<String>,
    pub ip: String,
    pub ban_type: String,
    pub reason: Option<String>,
    pub duration: String,
    pub status: String,
    pub admin_name: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub server_id: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, FromRow, ToSchema)]
pub struct PublicBan {
    pub id: i64,
    pub name: String,
    pub steam_id: String,
    pub steam_id_3: Option<String>,
    pub steam_id_64: Option<String>,
    pub reason: Option<String>,
    pub duration: String,
    pub status: String,
    pub admin_name: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateBanRequest {
    pub name: String,
    pub steam_id: Option<String>,
    pub steam_id_64: Option<String>,
    pub ip: String,
    pub ban_type: String,
    pub reason: Option<String>,
    pub duration: String,
    pub admin_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateBanRequest {
    pub name: Option<String>,
    pub steam_id: Option<String>,
    pub steam_id_64: Option<String>,
    pub ip: Option<String>,
    pub ban_type: Option<String>,
    pub reason: Option<String>,
    pub duration: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BanListQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PublicBanListQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub status: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PaginatedBanResponse {
    pub items: Vec<Ban>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PaginatedPublicBanResponse {
    pub items: Vec<PublicBan>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}
