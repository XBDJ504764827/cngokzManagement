use serde::{Deserialize, Serialize};
use regex::Regex;
use utoipa::ToSchema;

#[derive(Debug, Deserialize)]
struct SteamLevelResponse {
    response: SteamLevelData,
}

#[derive(Debug, Deserialize)]
struct SteamLevelData {
    player_level: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct OwnedGamesResponse {
    response: OwnedGamesData,
}

#[derive(Debug, Deserialize)]
struct OwnedGamesData {
    games: Option<Vec<SteamGame>>,
}

#[derive(Debug, Deserialize)]
struct SteamGame {
    appid: u32,
    playtime_forever: i32, // Minutes
}

#[derive(Debug, Deserialize)]
struct GokzPlayerResponse {
    rating: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ResolveVanityResponse {
    response: ResolveVanityData,
}

#[derive(Debug, Deserialize)]
struct ResolveVanityData {
    success: i32,
    steamid: Option<String>,
}
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct PlayerSummaryResponse {
    pub response: PlayerSummaryData,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct PlayerSummaryData {
    pub players: Vec<PlayerSummary>,
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
pub struct PlayerSummary {
    pub steamid: String,
    pub personaname: String,
    pub avatarfull: String,
    pub profileurl: String,
}


pub struct SteamService {
    client: reqwest::Client,
    api_key: String,
}

impl SteamService {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: std::env::var("STEAM_API_KEY").expect("STEAM_API_KEY must be set"),
        }
    }

    pub async fn get_steam_level(&self, steam_id_64: &str) -> Option<i32> {
        let url = format!(
            "https://api.steamchina.com/IPlayerService/GetSteamLevel/v0001/?key={}&steamid={}&include_appinfo=1",
            &self.api_key, steam_id_64
        );

        match self.client.get(&url).send().await {
            Ok(resp) => {
                if let Ok(data) = resp.json::<SteamLevelResponse>().await {
                    return data.response.player_level;
                }
            }
            Err(e) => tracing::error!("Steam API Level Error: {}", e),
        }
        None
    }

    pub async fn get_csgo_playtime_minutes(&self, steam_id_64: &str) -> Option<i32> {
        // CS:GO AppID = 730
        let url = format!(
            "https://api.steamchina.com/IPlayerService/GetOwnedGames/v0001/?key={}&steamid={}&include_appinfo=1&format=json",
            &self.api_key, steam_id_64
        );

        match self.client.get(&url).send().await {
            Ok(resp) => {
                if let Ok(data) = resp.json::<OwnedGamesResponse>().await {
                    if let Some(games) = data.response.games {
                        for game in games {
                            if game.appid == 730 {
                                return Some(game.playtime_forever);
                            }
                        }
                    }
                    // Games list returned but CSGO not found -> 0 minutes
                    return Some(0); 
                }
            }
            Err(e) => tracing::error!("Steam API Games Error: {}", e),
        }
        None
    }

    pub async fn get_gokz_rating(&self, steam_id_64: &str) -> Option<f64> {
        let url = format!("https://api.gokz.top/api/v1/players/{}", steam_id_64);
        match self.client.get(&url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    match resp.json::<GokzPlayerResponse>().await {
                        Ok(data) => return data.rating,
                        Err(e) => tracing::error!("Gokz API Parse Error: {}", e),
                    }
                } else {
                    tracing::warn!("Gokz API returned status: {}", resp.status());
                }
            }
            Err(e) => tracing::error!("Gokz API Request Error: {}", e),
        }
        None
    }

    /// Resolves various input formats to a SteamID64
    /// Supported:
    /// - SteamID64 (7656...)
    /// - SteamID2 (STEAM_0:1:123456)
    /// - SteamID3 ([U:1:123456])
    /// - Profile URL (.../profiles/7656...)
    /// - Custom URL (.../id/custom_name)
    /// - Vanity Name (custom_name)
    pub async fn resolve_steam_id(&self, input: &str) -> Option<String> {
        let input = input.trim();
        
        // 1. Check if it's already SteamID64 (17 digits, starts with 7)
        let re_id64 = Regex::new(r"^7656119\d{10}$").unwrap();
        if re_id64.is_match(input) {
            return Some(input.to_string());
        }

        // 2. Check SteamID2: STEAM_X:Y:Z
        // Magic: W=Z*2+Y, ID64 = W + 76561197960265728
        let re_id2 = Regex::new(r"^STEAM_[0-5]:([01]):(\d+)$").unwrap();
        if let Some(caps) = re_id2.captures(input) {
            let y: u64 = caps[1].parse().ok()?;
            let z: u64 = caps[2].parse().ok()?;
            let w = z * 2 + y;
            let id64 = w + 76561197960265728;
            return Some(id64.to_string());
        }

        // 3. Check SteamID3: [U:1:AccountID]
        let re_id3 = Regex::new(r"^\[U:1:(\d+)\]$").unwrap();
        if let Some(caps) = re_id3.captures(input) {
            let account_id: u64 = caps[1].parse().ok()?;
            let id64 = account_id + 76561197960265728;
            return Some(id64.to_string());
        }

        // 4. Handle URLs
        if input.contains("steamcommunity.com") {
            // Remove trailing slash
            let clean_url = input.trim_end_matches('/');
            
            // .../profiles/ID64
            if clean_url.contains("/profiles/") {
                if let Some(pos) = clean_url.rfind('/') {
                    let id_part = &clean_url[pos+1..];
                    if re_id64.is_match(id_part) {
                        return Some(id_part.to_string());
                    }
                }
            }
            
            // .../id/VANITY
            if clean_url.contains("/id/") {
                if let Some(pos) = clean_url.rfind('/') {
                    let vanity = &clean_url[pos+1..];
                    return self.resolve_vanity_url(vanity).await;
                }
            }
        }

        // 5. Fallback: Assume it might be a vanity URL name directly if it looks like one (alphanumeric)
        // Avoid purely numeric ones unless they failed ID64 check (which they did to get here)
        // But be careful not to resolve garbage.
        let re_vanity = Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap();
        if re_vanity.is_match(input) {
            return self.resolve_vanity_url(input).await;
        }

        None
    }

    async fn resolve_vanity_url(&self, vanity_url: &str) -> Option<String> {
        let url = format!(
            "https://api.steampowered.com/ISteamUser/ResolveVanityURL/v0001/?key={}&vanityurl={}",
            &self.api_key, vanity_url
        );
        
        match self.client.get(&url).send().await {
            Ok(resp) => {
                if let Ok(data) = resp.json::<ResolveVanityResponse>().await {
                    if data.response.success == 1 {
                        return data.response.steamid;
                    }
                }
            }
            Err(e) => tracing::error!("Steam Vanity Resolve Error: {}", e),
        }
        None
    }

    /// Converts a SteamID64 string to SteamID2 format (STEAM_0:Y:Z)
    pub fn id64_to_id2(&self, steam_id_64: &str) -> Option<String> {
        let id64: u64 = steam_id_64.parse().ok()?;
        let base_num = 76561197960265728u64;
        
        if id64 < base_num {
            return None;
        }

        let w = id64 - base_num;
        let y = w % 2;
        let z = (w - y) / 2;

        // Note: Universe is usually 0 or 1. Historically STEAM_0:..., but modern often STEAM_1:...
        // However, in DBs/Plugins usually STEAM_1 is standard for CSGO, but STEAM_0 is legacy.
        // Let's standardise on STEAM_1 unless it's very old? 
        // Actually, Valve wiki says "STEAM_X:Y:Z", X is universe. Public universe is 1. 
        // BUT old games displayed STEAM_0. Let's stick to STEAM_1 for CSGO context if safe, 
        // OR better yet, check if we need to support both.
        // Given 'STEAM_0:1:783986425' example in prompt, user uses STEAM_0 ??
        // Let's look at the example prompt: "STEAM_0:1:783986425". 
        // OK, user specifically asked for STEAM_0. I will use STEAM_0.
        Some(format!("STEAM_0:{}:{}", y, z))
    }

    /// Converts a SteamID64 string to SteamID3 format ([U:1:AccountID])
    pub fn id64_to_id3(&self, steam_id_64: &str) -> Option<String> {
        let id64: u64 = steam_id_64.parse().ok()?;
        let base_num = 76561197960265728u64;
        
        if id64 < base_num {
            return None;
        }

        let account_id = id64 - base_num;
        Some(format!("[U:1:{}]", account_id))
    }
    pub async fn get_player_summary(&self, steam_id_64: &str) -> Option<PlayerSummary> {
        let url = format!(
            "https://api.steampowered.com/ISteamUser/GetPlayerSummaries/v0002/?key={}&steamids={}",
            &self.api_key, steam_id_64
        );

        match self.client.get(&url).send().await {
            Ok(resp) => {
                if let Ok(data) = resp.json::<PlayerSummaryResponse>().await {
                    if let Some(player) = data.response.players.first() {
                        return Some(player.clone());
                    }
                }
            }
            Err(e) => tracing::error!("Steam API Summary Error: {}", e),
        }
        None
    }
}
