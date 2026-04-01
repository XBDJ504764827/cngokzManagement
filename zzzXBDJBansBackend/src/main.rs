use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
use dotenvy::dotenv;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tower_http::cors::CorsLayer;

mod db;
mod handlers;
mod models;
mod middleware;
mod utils;
mod bg_task;
mod services;

#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::auth::login,
        handlers::auth::logout,
        handlers::auth::me,
        handlers::auth::change_password,
        handlers::admin::list_admins,
        handlers::admin::create_admin,
        handlers::admin::update_admin,
        handlers::admin::delete_admin,
        handlers::ban::list_bans,
        handlers::ban::list_public_bans,
        handlers::ban::check_ban,
        handlers::ban::create_ban,
        handlers::ban::update_ban,
        handlers::ban::delete_ban,
        handlers::whitelist::list_whitelist,
        handlers::whitelist::list_pending,
        handlers::whitelist::list_rejected,
        handlers::whitelist::apply_whitelist,
        handlers::whitelist::create_whitelist,
        handlers::whitelist::approve_whitelist,
        handlers::whitelist::reject_whitelist,
        handlers::whitelist::delete_whitelist,
        handlers::whitelist::list_public_whitelist,
        handlers::whitelist::get_player_info,
        handlers::server::list_server_groups,
        handlers::server::create_group,
        handlers::server::delete_group,
        handlers::server::create_server,
        handlers::server::update_server,
        handlers::server::delete_server,
        handlers::server::check_server_status,
        handlers::server::get_server_players,
        handlers::server::kick_player,
        handlers::server::ban_player,
        handlers::log::list_logs,
        handlers::log::create_log,
        handlers::verification::list_verifications,
        handlers::verification::create_verification,
        handlers::verification::update_verification,
        handlers::verification::delete_verification,
    ),
    components(
        schemas(
            models::user::Admin,
            models::user::CreateAdminRequest,
            models::user::UpdateAdminRequest,
            models::user::LoginRequest,
            models::user::LoginResponse,
            models::user::ChangePasswordRequest,
            models::ban::Ban,
            models::ban::PublicBan,
            models::ban::CreateBanRequest,
            models::ban::CreateBanRequest,
            models::ban::UpdateBanRequest,
            models::whitelist::Whitelist,
            models::whitelist::CreateWhitelistRequest,
            models::whitelist::ApplyWhitelistRequest,
            crate::services::steam_api::PlayerSummary,
            models::server::ServerGroup,
            models::server::GroupWithServers,
            models::server::Server,
            models::server::CreateGroupRequest,
            models::server::CreateServerRequest,
            models::server::UpdateServerRequest,
            models::server::CheckServerRequest,
            handlers::server::Player,
            handlers::server::KickPlayerRequest,
            handlers::server::BanPlayerRequest,
            models::log::AuditLog,
            models::log::CreateLogRequest,
            handlers::verification::VerificationRecord,
            handlers::verification::CreateVerificationRequest,
            handlers::verification::UpdateVerificationRequest,
        )
    ),
    tags(
        (name = "zzzXBDJBans", description = "Backend API")
    ),
    modifiers(&SecurityAddon)
)]
struct ApiDoc;

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "jwt",
                utoipa::openapi::security::SecurityScheme::Http(
                    utoipa::openapi::security::Http::new(
                        utoipa::openapi::security::HttpAuthScheme::Bearer,
                    ),
                ),
            )
        }
    }
}

// Application State
pub struct AppState {
    pub db: sqlx::MySqlPool,
    pub client: reqwest::Client,
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::fmt::init();

    let pool = db::establish_connection().await;

    // FIX: Always remove the known broken migration record to prevent VersionMismatch on different envs
    tracing::info!("Attempting to clear specific migration records...");
    
    match sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 20260216190000").execute(&pool).await {
        Ok(_) => tracing::info!("Cleared migration 20260216190000"),
        Err(e) => tracing::warn!("Failed to clear 20260216190000: {}", e),
    }

    // Force re-run init migration if admins table is missing
    let table_exists: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_NAME = 'admins' AND TABLE_SCHEMA = DATABASE()")
        .fetch_one(&pool).await.unwrap_or((0,));
    if table_exists.0 == 0 {
        tracing::info!("'admins' table missing, forcing re-run of init migration...");
        let _ = sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 20260125000000").execute(&pool).await;
    }
    
    match sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 20260217130000").execute(&pool).await {
         Ok(_) => tracing::info!("Cleared dirty migration 20260217130000"),
         Err(e) => tracing::warn!("Failed to clear 20260217130000: {}", e),
    }

    match sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 20260125020000").execute(&pool).await {
         Ok(done) => tracing::info!("Cleared dirty migration 20260125020000. Rows affected: {}", done.rows_affected()),
         Err(e) => tracing::error!("Failed to clear dirty migration 20260125020000: {}", e),
    }

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    ensure_super_admin(&pool).await;

    let state = Arc::new(AppState { 
        db: pool,
        client: reqwest::Client::new(),
    });

    // Spawn background task FIRST, cloning state
    let task_state = state.clone();
    tokio::spawn(async move {
        crate::bg_task::start_background_task(task_state).await;
    });

    let verif_state = state.clone();
    tokio::spawn(async move {
        crate::services::verification_worker::start_verification_worker(verif_state.db.clone()).await;
    });

    let protected_routes = Router::new()
        .route("/api/auth/me", get(handlers::auth::me))
        .route("/api/auth/logout", axum::routing::post(handlers::auth::logout))
        // Admins
        .route("/api/admins", get(handlers::admin::list_admins).post(handlers::admin::create_admin))
        .route("/api/admins/:id", axum::routing::put(handlers::admin::update_admin).delete(handlers::admin::delete_admin))
        // Bans
        .route("/api/bans", get(handlers::ban::list_bans).post(handlers::ban::create_ban))
        .route("/api/bans/:id", axum::routing::put(handlers::ban::update_ban).delete(handlers::ban::delete_ban))
        .route("/api/check_ban", get(handlers::ban::check_ban))
        .route("/api/check_global_ban", get(handlers::ban::check_global_ban))
        .route("/api/check_global_ban/bulk", post(handlers::ban::check_global_ban_bulk))
        // Logs
        .route("/api/logs", get(handlers::log::list_logs).post(handlers::log::create_log))

        // Whitelist (管理员操作)
        .route("/api/whitelist", get(handlers::whitelist::list_whitelist).post(handlers::whitelist::create_whitelist))
        .route("/api/whitelist/pending", get(handlers::whitelist::list_pending))
        .route("/api/whitelist/rejected", get(handlers::whitelist::list_rejected))
        .route("/api/whitelist/:id", axum::routing::delete(handlers::whitelist::delete_whitelist))
        .route("/api/whitelist/:id/approve", axum::routing::put(handlers::whitelist::approve_whitelist))
        .route("/api/whitelist/:id/reject", axum::routing::put(handlers::whitelist::reject_whitelist))

        // Verifications (Manual)
        .route("/api/verifications", get(handlers::verification::list_verifications).post(handlers::verification::create_verification))
        .route("/api/verifications/:id", axum::routing::put(handlers::verification::update_verification).delete(handlers::verification::delete_verification))

        // Server Management
        .route("/api/server-groups", get(handlers::server::list_server_groups).post(handlers::server::create_group))
        .route("/api/server-groups/:id", axum::routing::delete(handlers::server::delete_group))
        .route("/api/servers", axum::routing::post(handlers::server::create_server))
        .route("/api/servers/:id", axum::routing::put(handlers::server::update_server).delete(handlers::server::delete_server))
        .route("/api/servers/check", axum::routing::post(handlers::server::check_server_status))
        // Player Management
        .route("/api/servers/:id/players", get(handlers::server::get_server_players))
        .route("/api/servers/:id/kick", axum::routing::post(handlers::server::kick_player))
        .route("/api/servers/:id/ban", axum::routing::post(handlers::server::ban_player))
        .route_layer(axum::middleware::from_fn(middleware::auth_middleware));

    let app = Router::new()
        .route("/", get(root))
        .route("/api/auth/login", axum::routing::post(handlers::auth::login))
        .route("/api/auth/change-password", axum::routing::post(handlers::auth::change_password))
        // 公开路由：白名单申请（无需认证）
        .route("/api/whitelist/apply", axum::routing::post(handlers::whitelist::apply_whitelist))
        .route("/api/whitelist/public-list", get(handlers::whitelist::list_public_whitelist))
        .route("/api/whitelist/player-info", get(handlers::whitelist::get_player_info))
        .route("/api/bans/public", get(handlers::ban::list_public_bans))
        .merge(protected_routes)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("SERVER_PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("{}:{}", host, port).parse::<SocketAddr>().expect("Invalid address");

    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn root() -> &'static str {
    "zzzXBDJBans Backend API"
}

async fn ensure_super_admin(pool: &sqlx::MySqlPool) {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admins")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    if count == 0 {
        tracing::info!("No admins found. Creating default super_admin.");
        let username = "admin";
        let password = "123456"; 
        let hashed = bcrypt::hash(password, bcrypt::DEFAULT_COST).expect("Failed to hash password");
        
        let _ = sqlx::query(
            "INSERT INTO admins (username, password, role) VALUES (?, ?, 'super_admin')"
        )
        .bind(username)
        .bind(hashed)
        .execute(pool)
        .await
        .expect("Failed to create default admin");
        
        tracing::info!("Default admin created: user='admin', pass='123456'");
    } else {
        tracing::info!("Super admin exists. Skipping creation.");
    }
}
