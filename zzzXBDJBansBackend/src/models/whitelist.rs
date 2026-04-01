use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Whitelist {
    pub id: i64,
    pub steam_id: String,
    pub steam_id_3: Option<String>,
    pub steam_id_64: Option<String>,
    pub name: String,
    pub status: String, // 'approved', 'pending', 'rejected'
    pub reject_reason: Option<String>,
    pub admin_name: Option<String>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateWhitelistRequest {
    pub steam_id: String,
    pub name: String,
}

// 玩家申请白名单的请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct ApplyWhitelistRequest {
    pub steam_id: String,
    pub name: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RejectWhitelistRequest {
    pub reason: String,
}
