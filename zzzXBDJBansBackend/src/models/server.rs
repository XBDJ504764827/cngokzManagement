use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, FromRow, ToSchema)]
pub struct ServerGroup {
    pub id: i64,
    pub name: String,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, FromRow, Clone, ToSchema)]
pub struct Server {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub ip: String,
    pub port: i32,
    pub rcon_password: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    #[sqlx(default)]
    pub verification_enabled: bool,
    #[sqlx(default)]
    pub cached_status: String,
    pub status_checked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct ServerSummary {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub ip: String,
    pub port: i32,
    pub created_at: Option<DateTime<Utc>>,
    pub verification_enabled: bool,
    pub status: String,
    pub status_checked_at: Option<DateTime<Utc>>,
}

// Responses often group servers by group
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GroupWithServers {
    pub id: i64,
    pub name: String,
    pub servers: Vec<ServerSummary>,
}

#[derive(Debug, Serialize, Deserialize, FromRow, ToSchema)]
pub struct ServerStatusSummary {
    pub server_id: i64,
    pub status: String,
    pub status_checked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateGroupRequest {
    pub name: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateServerRequest {
    pub group_id: i64,
    pub name: String,
    pub ip: String,
    pub port: i32,
    pub rcon_password: Option<String>,
    pub verification_enabled: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateServerRequest {
    pub name: Option<String>,
    pub ip: Option<String>,
    pub port: Option<i32>,
    pub rcon_password: Option<String>,
    pub verification_enabled: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CheckServerRequest {
    pub ip: String,
    pub port: u16,
    pub rcon_password: Option<String>,
}
