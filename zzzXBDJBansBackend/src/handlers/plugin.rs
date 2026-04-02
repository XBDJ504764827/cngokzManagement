use std::{env, sync::Arc};

use axum::{
    extract::{Form, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sqlx::Row;
use utoipa::ToSchema;

use crate::AppState;

const DEFAULT_REQUIRED_RATING: f64 = 3.0;
const DEFAULT_REQUIRED_LEVEL: i32 = 1;
const DEFAULT_RETRY_AFTER_SECONDS: u32 = 2;
const PLUGIN_TOKEN_HEADER: &str = "x-plugin-token";

#[derive(Debug, Deserialize, ToSchema)]
pub struct PluginAccessRequest {
    pub server_id: i64,
    pub steam_id_64: String,
    pub steam_id: Option<String>,
    pub player_name: Option<String>,
    pub ip_address: String,
}

struct PluginServer {
    id: i64,
    verification_enabled: bool,
}

struct WhitelistDecision {
    status: String,
    reject_reason: Option<String>,
}

struct CacheRecord {
    status: String,
    steam_level: Option<i32>,
    gokz_rating: Option<f64>,
}

struct BanDecision {
    ban_id: i64,
    reason: Option<String>,
    duration: String,
    stored_ip: String,
    ban_type: String,
    banned_steam_id_64: Option<String>,
    server_id: Option<i64>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[utoipa::path(
    post,
    path = "/api/plugin/access-check",
    request_body(content = PluginAccessRequest, content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 200, description = "Plain-text access decision"),
        (status = 401, description = "Invalid plugin token"),
        (status = 404, description = "Server not found")
    )
)]
pub async fn access_check(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Form(payload): Form<PluginAccessRequest>,
) -> impl IntoResponse {
    if let Err(response) = authorize_plugin(&headers) {
        return response;
    }

    let server = match load_server(&state, payload.server_id).await {
        Ok(Some(server)) => server,
        Ok(None) => {
            return plain_text_response(StatusCode::NOT_FOUND, "deny", "服务器未注册", 0);
        }
        Err(e) => {
            tracing::error!("Plugin access check failed to load server: {}", e);
            return plain_text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "deny",
                "验证服务异常",
                0,
            );
        }
    };

    let steam_id_64 = payload.steam_id_64.trim().to_string();
    if steam_id_64.is_empty() {
        return plain_text_response(StatusCode::BAD_REQUEST, "deny", "无效的 SteamID64", 0);
    }

    let steam_id = payload.steam_id.unwrap_or_default().trim().to_string();
    let other_steam_id = alternate_steam_id(&steam_id);
    let player_name = payload.player_name.unwrap_or_default();
    let ip_address = payload.ip_address.trim().to_string();

    if server.verification_enabled {
        match whitelist_decision(&state, &steam_id_64, &steam_id, &other_steam_id).await {
            Ok(Some(record)) if record.status == "rejected" => {
                let message = record
                    .reject_reason
                    .unwrap_or_else(|| "您已被拒绝访问本服务器".to_string());
                return plain_text_response(StatusCode::OK, "deny", &message, 0);
            }
            Ok(Some(record)) if record.status == "approved" => {}
            Ok(_) => {
                match evaluate_verification(&state, &steam_id_64, &player_name, &ip_address).await {
                    Ok(VerificationFlow::Allow) => {}
                    Ok(VerificationFlow::Pending) => {
                        return plain_text_response(
                            StatusCode::OK,
                            "pending",
                            "验证中，请稍候重试",
                            DEFAULT_RETRY_AFTER_SECONDS,
                        );
                    }
                    Ok(VerificationFlow::Deny(message)) => {
                        return plain_text_response(StatusCode::OK, "deny", &message, 0);
                    }
                    Err(e) => {
                        tracing::error!("Plugin verification flow failed: {}", e);
                        return plain_text_response(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "deny",
                            "验证服务异常",
                            0,
                        );
                    }
                }
            }
            Err(e) => {
                tracing::error!("Plugin whitelist flow failed: {}", e);
                return plain_text_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "deny",
                    "白名单服务异常",
                    0,
                );
            }
        }
    }

    match evaluate_ban(
        &state,
        server.id,
        &steam_id_64,
        &steam_id,
        &other_steam_id,
        &ip_address,
        &player_name,
    )
    .await
    {
        Ok(Some(message)) => plain_text_response(StatusCode::OK, "deny", &message, 0),
        Ok(None) => plain_text_response(StatusCode::OK, "allow", "OK", 0),
        Err(e) => {
            tracing::error!("Plugin ban flow failed: {}", e);
            plain_text_response(StatusCode::INTERNAL_SERVER_ERROR, "deny", "封禁服务异常", 0)
        }
    }
}

fn authorize_plugin(headers: &HeaderMap) -> Result<(), Response> {
    let expected = match env::var("PLUGIN_API_TOKEN") {
        Ok(token) if !token.trim().is_empty() => token,
        _ => {
            tracing::error!("PLUGIN_API_TOKEN is not configured");
            return Err(plain_text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "deny",
                "验证服务未配置令牌",
                0,
            ));
        }
    };

    let provided = headers
        .get(PLUGIN_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    if provided == expected {
        Ok(())
    } else {
        Err(plain_text_response(
            StatusCode::UNAUTHORIZED,
            "deny",
            "无效的插件令牌",
            0,
        ))
    }
}

async fn load_server(
    state: &Arc<AppState>,
    server_id: i64,
) -> Result<Option<PluginServer>, sqlx::Error> {
    let row = sqlx::query("SELECT id, verification_enabled FROM servers WHERE id = $1")
        .bind(server_id)
        .fetch_optional(&state.db)
        .await?;

    Ok(row.map(|row| PluginServer {
        id: row.get("id"),
        verification_enabled: row.get("verification_enabled"),
    }))
}

async fn whitelist_decision(
    state: &Arc<AppState>,
    steam_id_64: &str,
    steam_id: &str,
    other_steam_id: &str,
) -> Result<Option<WhitelistDecision>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT status, reject_reason
         FROM whitelist
         WHERE steam_id_64 = $1 OR steam_id = $2 OR steam_id = $3
         ORDER BY CASE status WHEN 'approved' THEN 0 WHEN 'rejected' THEN 1 ELSE 2 END
         LIMIT 1",
    )
    .bind(steam_id_64)
    .bind(steam_id)
    .bind(other_steam_id)
    .fetch_optional(&state.db)
    .await?;

    Ok(row.map(|row| WhitelistDecision {
        status: row.get("status"),
        reject_reason: row.get("reject_reason"),
    }))
}

enum VerificationFlow {
    Allow,
    Pending,
    Deny(String),
}

async fn evaluate_verification(
    state: &Arc<AppState>,
    steam_id_64: &str,
    player_name: &str,
    ip_address: &str,
) -> Result<VerificationFlow, sqlx::Error> {
    let row = sqlx::query(
        "SELECT status, reason, steam_level, gokz_rating
         FROM player_cache
         WHERE steam_id = $1",
    )
    .bind(steam_id_64)
    .fetch_optional(&state.db)
    .await?;

    let Some(row) = row else {
        queue_verification(state, steam_id_64, player_name, ip_address).await?;
        return Ok(VerificationFlow::Pending);
    };

    let record = CacheRecord {
        status: row.get("status"),
        steam_level: row.get("steam_level"),
        gokz_rating: row.get("gokz_rating"),
    };

    match record.status.as_str() {
        "allowed" => Ok(VerificationFlow::Allow),
        "pending" => {
            refresh_verification_metadata(state, steam_id_64, player_name, ip_address).await?;
            Ok(VerificationFlow::Pending)
        }
        "verified" => {
            let level = record.steam_level.unwrap_or(0);
            let rating = record.gokz_rating.unwrap_or(0.0);
            let required_rating = required_rating();
            let required_level = required_level();

            if rating >= required_rating && level >= required_level {
                let reason = format!("验证通过：Rating {:.2} / 等级 {}", rating, level);
                sqlx::query(
                    "UPDATE player_cache
                     SET status = 'allowed', reason = $1, updated_at = NOW()
                     WHERE steam_id = $2",
                )
                .bind(&reason)
                .bind(steam_id_64)
                .execute(&state.db)
                .await?;

                Ok(VerificationFlow::Allow)
            } else {
                sqlx::query("DELETE FROM player_cache WHERE steam_id = $1")
                    .bind(steam_id_64)
                    .execute(&state.db)
                    .await?;

                let message = format!(
                    "验证失败：Rating {:.2}(需>={:.1}) / 等级 {}(需>={})",
                    rating, required_rating, level, required_level
                );
                Ok(VerificationFlow::Deny(message))
            }
        }
        _ => {
            queue_verification(state, steam_id_64, player_name, ip_address).await?;
            Ok(VerificationFlow::Pending)
        }
    }
}

async fn queue_verification(
    state: &Arc<AppState>,
    steam_id_64: &str,
    player_name: &str,
    ip_address: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO player_cache (steam_id, player_name, ip_address, status)
         VALUES ($1, $2, $3, 'pending')
         ON CONFLICT (steam_id) DO UPDATE
         SET player_name = EXCLUDED.player_name,
             ip_address = EXCLUDED.ip_address,
             status = 'pending',
             reason = NULL,
             steam_level = NULL,
             playtime_minutes = NULL,
             gokz_rating = NULL,
             updated_at = NOW()",
    )
    .bind(steam_id_64)
    .bind(player_name)
    .bind(ip_address)
    .execute(&state.db)
    .await?;

    Ok(())
}

async fn refresh_verification_metadata(
    state: &Arc<AppState>,
    steam_id_64: &str,
    player_name: &str,
    ip_address: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE player_cache
         SET player_name = $1, ip_address = $2, updated_at = NOW()
         WHERE steam_id = $3",
    )
    .bind(player_name)
    .bind(ip_address)
    .bind(steam_id_64)
    .execute(&state.db)
    .await?;

    Ok(())
}

async fn evaluate_ban(
    state: &Arc<AppState>,
    server_id: i64,
    steam_id_64: &str,
    steam_id: &str,
    other_steam_id: &str,
    ip_address: &str,
    player_name: &str,
) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, reason, duration, ip, ban_type, steam_id_64, server_id, expires_at
         FROM bans
         WHERE (steam_id_64 = $1 OR steam_id = $2 OR steam_id = $3 OR ip = $4)
           AND status = 'active'
           AND (expires_at IS NULL OR expires_at > NOW())
         ORDER BY id DESC
         LIMIT 1",
    )
    .bind(steam_id_64)
    .bind(steam_id)
    .bind(other_steam_id)
    .bind(ip_address)
    .fetch_optional(&state.db)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let decision = BanDecision {
        ban_id: row.get("id"),
        reason: row.get("reason"),
        duration: row.get("duration"),
        stored_ip: row.get("ip"),
        ban_type: row.get("ban_type"),
        banned_steam_id_64: row.get("steam_id_64"),
        server_id: row.get("server_id"),
        expires_at: row.get("expires_at"),
    };

    let is_same_account = decision
        .banned_steam_id_64
        .as_deref()
        .map(|value| value == steam_id_64)
        .unwrap_or(false);

    if is_same_account {
        if decision.stored_ip.trim().is_empty() {
            let _ = sqlx::query("UPDATE bans SET ip = $1 WHERE id = $2")
                .bind(ip_address)
                .bind(decision.ban_id)
                .execute(&state.db)
                .await;
        }

        let message = format!(
            "您已被封禁。原因：{}（时长：{}）",
            decision.reason.unwrap_or_else(|| "无".to_string()),
            decision.duration
        );
        return Ok(Some(message));
    }

    if decision.ban_type == "ip" {
        let linked_reason = format!(
            "同IP关联封禁 (Linked to {})",
            decision
                .banned_steam_id_64
                .unwrap_or_else(|| "unknown".to_string())
        );
        let steam_id_value = if steam_id.is_empty() {
            "PENDING"
        } else {
            steam_id
        };

        let _ = sqlx::query(
            "INSERT INTO bans (
                name, steam_id, steam_id_64, ip, ban_type, reason, duration,
                admin_name, expires_at, created_at, status, server_id
             ) VALUES ($1, $2, $3, $4, 'account', $5, $6, $7, $8, NOW(), 'active', $9)",
        )
        .bind(player_name)
        .bind(steam_id_value)
        .bind(steam_id_64)
        .bind(ip_address)
        .bind(&linked_reason)
        .bind(&decision.duration)
        .bind("System (IP Linked)")
        .bind(decision.expires_at)
        .bind(decision.server_id.unwrap_or(server_id))
        .execute(&state.db)
        .await;

        return Ok(Some(
            "检测到关联封禁 IP。在此 IP 上的所有账号均被禁止进入。".to_string(),
        ));
    }

    Ok(None)
}

fn alternate_steam_id(steam_id: &str) -> String {
    if steam_id.len() <= 6 || !steam_id.starts_with("STEAM_") {
        return steam_id.to_string();
    }

    let mut chars: Vec<char> = steam_id.chars().collect();
    chars[6] = match chars[6] {
        '0' => '1',
        '1' => '0',
        other => other,
    };
    chars.into_iter().collect()
}

fn required_rating() -> f64 {
    env::var("PLUGIN_REQUIRED_RATING")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(DEFAULT_REQUIRED_RATING)
}

fn required_level() -> i32 {
    env::var("PLUGIN_REQUIRED_LEVEL")
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(DEFAULT_REQUIRED_LEVEL)
}

fn plain_text_response(
    status: StatusCode,
    action: &str,
    message: &str,
    retry_after_seconds: u32,
) -> Response {
    let sanitized_message = message.replace('\n', " ").replace('\r', " ");
    let body = format!(
        "action={}\nretry_after={}\nmessage={}\n",
        action, retry_after_seconds, sanitized_message
    );

    let mut response = (status, body).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}
