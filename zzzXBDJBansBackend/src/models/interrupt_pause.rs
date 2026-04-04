use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, FromRow, ToSchema)]
pub struct InterruptPauseSnapshot {
    pub id: i64,
    pub server_id: i64,
    pub server_name: Option<String>,
    pub server_ip: Option<String>,
    pub server_port: Option<i32>,
    pub auth_primary: String,
    pub auth_steamid64: Option<String>,
    pub auth_steam3: Option<String>,
    pub auth_steam2: Option<String>,
    pub auth_engine: Option<String>,
    pub player_name: String,
    pub ip_address: String,
    pub map_name: String,
    pub mode: i32,
    pub course: i32,
    pub time_seconds: f64,
    pub checkpoint_count: i32,
    pub teleport_count: i32,
    pub storage_version: i32,
    pub restore_status: String,
    pub restore_requested_at: Option<DateTime<Utc>>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub reviewed_by: Option<String>,
    pub reject_reason: Option<String>,
    pub restored_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RejectInterruptPauseRequest {
    pub reason: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PluginInterruptPauseSaveRequest {
    pub server_id: i64,
    pub auth_primary: String,
    pub auth_steamid64: Option<String>,
    pub auth_steam3: Option<String>,
    pub auth_steam2: Option<String>,
    pub auth_engine: Option<String>,
    pub player_name: Option<String>,
    pub ip_address: String,
    pub map_name: String,
    pub mode: i32,
    pub course: i32,
    pub time_seconds: f64,
    pub checkpoint_count: i32,
    pub teleport_count: i32,
    pub storage_version: i32,
    pub payload: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PluginInterruptPauseLookupRequest {
    pub server_id: i64,
    pub auth_primary: Option<String>,
    pub auth_steamid64: Option<String>,
    pub auth_steam3: Option<String>,
    pub auth_steam2: Option<String>,
    pub auth_engine: Option<String>,
}
