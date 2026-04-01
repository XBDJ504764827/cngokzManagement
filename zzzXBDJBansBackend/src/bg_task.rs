use std::sync::Arc;
use crate::AppState;
use tokio::time::{interval, Duration};
use crate::models::server::Server;
use crate::models::ban::Ban;
use crate::utils::rcon::send_command;


pub async fn start_background_task(state: Arc<AppState>) {
    tracing::info!("Background Task Started: Player IP Enforcement");
    let mut interval = interval(Duration::from_secs(60));

    loop {
        interval.tick().await;
        if let Err(e) = check_all_servers(&state).await {
            tracing::error!("Background Task Error: {}", e);
        }
    }
}

async fn check_all_servers(state: &Arc<AppState>) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Get all Active IP Bans
    let ip_bans = sqlx::query_as::<_, Ban>(
        "SELECT * FROM bans WHERE status = 'active' AND ban_type = 'ip'"
    )
    .fetch_all(&state.db)
    .await?;

    if ip_bans.is_empty() {
        return Ok(());
    }

    // Convert to HashMap for fast lookup: IP -> Ban Details
    // Note: Multiple bans might exist for same IP (though unlikely if managed well), we just need one valid one.
    use std::collections::{HashMap, HashSet};
    let ip_ban_map: HashMap<String, Ban> = ip_bans.into_iter()
        .map(|b| (b.ip.clone(), b))
        .collect();

    // 2. Get all Active Account Bans (SteamIDs) to avoid N+1 DB check
    // We only need the steam_ids to know if they are already banned.
    let account_bans_result = sqlx::query!(
        "SELECT steam_id FROM bans WHERE status = 'active' AND steam_id IS NOT NULL"
    )
    .fetch_all(&state.db)
    .await?;

    let mut active_steamids: HashSet<String> = account_bans_result.into_iter()
        .map(|r| r.steam_id.clone())
        .collect();

    // 3. Get Servers
    let servers = sqlx::query_as::<_, Server>("SELECT * FROM servers")
        .fetch_all(&state.db)
        .await?;

    // 4. Check each server
    for server in servers {
        let address = format!("{}:{}", server.ip, server.port);
        let pwd = server.rcon_password.unwrap_or_default();

        match send_command(&address, &pwd, "status").await {
            Ok(output) => {
                for line in output.lines() {
                    let line = line.trim();
                    if !line.starts_with("#") { continue; }
                    
                    // Parse logic...
                    let first_quote = match line.find('"') {
                        Some(idx) => idx,
                        None => continue,
                    };
                    let last_quote = match line.rfind('"') {
                        Some(idx) => idx,
                        None => continue,
                    };

                    if first_quote >= last_quote { continue; }

                    let pre_name = &line[..first_quote].trim();
                    let pre_parts: Vec<&str> = pre_name.split_whitespace().collect();
                    let userid = pre_parts.last().unwrap_or(&"");

                    if *userid == "#" || userid.is_empty() { continue; }

                    let player_name = &line[first_quote+1..last_quote]; 
                    let after_name = &line[last_quote+1..].trim(); 
                    let fields: Vec<&str> = after_name.split_whitespace().collect();
                    
                    if fields.len() < 2 { continue; }
                    
                    let steam_id = fields[0];
                    let ip_port = fields.last().unwrap_or(&"");
                    let ip_only = ip_port.split(':').next().unwrap_or("");
                    
                    if ip_only.is_empty() || steam_id == "BOT" { continue; }

                    // CHECK: Is this IP in our ban list?
                    if let Some(ban) = ip_ban_map.get(ip_only) {
                        // IP is BANNED. Check if Account is already banned.
                        if active_steamids.contains(steam_id) {
                            // Already banned - Just Kick
                            let _ = send_command(&address, &pwd, &format!("kickid {} \"Banned IP Detected\"", userid)).await;
                        } else {
                            // NEW CATCH!
                            tracing::info!("BG Task: Caught user bypassing IP Ban! IP: {}, SteamID: {}, Name: {}", ip_only, steam_id, player_name);
                            
                            let reason = "同IP关联封禁 (Detected online with Banned IP)";
                            let expires_at = ban.expires_at;

                            // Insert into DB
                            let insert_result = sqlx::query(
                                "INSERT INTO bans (name, steam_id, ip, ban_type, reason, duration, admin_name, expires_at, created_at, status, server_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, NOW(), 'active', ?)"
                            )
                            .bind(player_name)
                            .bind(steam_id)
                            .bind(ip_only)
                            .bind("account")
                            .bind(reason)
                            .bind(&ban.duration)
                            .bind("System (BG Monitor)")
                            .bind(expires_at)
                            .bind(server.id) // Use current server ID logic
                            .execute(&state.db)
                            .await;

                            if let Ok(_) = insert_result {
                                // Add to local cache so we don't try to ban again in this loop
                                active_steamids.insert(steam_id.to_string());
                            }

                            // Ban & Kick on Server
                            let duration_str = &ban.duration;
                            let _ = send_command(&address, &pwd, &format!("sm_ban #{} {} \"{}\"", userid, duration_str, reason)).await;
                        }
                    }
                }
            },
            Err(_) => continue, 
        }
    }

    Ok(())
}
