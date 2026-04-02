use futures::stream::{self, StreamExt};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    env,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct GlobalBanCacheService {
    client: reqwest::Client,
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    ttl: Duration,
    concurrency_limit: usize,
}

#[derive(Clone)]
struct CacheEntry {
    value: Option<Value>,
    expires_at: Instant,
}

impl GlobalBanCacheService {
    pub fn new(client: reqwest::Client) -> Self {
        let ttl_seconds = env::var("GLOBAL_BAN_CACHE_TTL_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(300);

        let concurrency_limit = env::var("GLOBAL_BAN_FETCH_CONCURRENCY")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(8)
            .max(1);

        Self {
            client,
            cache: Arc::new(RwLock::new(HashMap::new())),
            ttl: Duration::from_secs(ttl_seconds),
            concurrency_limit,
        }
    }

    pub async fn get_ban(&self, steam_id: &str) -> Option<Value> {
        self.get_bans(vec![steam_id.to_string()])
            .await
            .remove(steam_id)
            .flatten()
    }

    pub async fn get_bans(&self, steam_ids: Vec<String>) -> HashMap<String, Option<Value>> {
        let unique_ids: Vec<String> = steam_ids
            .into_iter()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        let mut results = HashMap::new();
        let mut missing_ids = Vec::new();
        let now = Instant::now();

        {
            let cache = self.cache.read().await;
            for steam_id in &unique_ids {
                if let Some(entry) = cache.get(steam_id) {
                    if entry.expires_at > now {
                        results.insert(steam_id.clone(), entry.value.clone());
                        continue;
                    }
                }

                missing_ids.push(steam_id.clone());
            }
        }

        if missing_ids.is_empty() {
            return results;
        }

        let fetched = stream::iter(missing_ids.into_iter().map(|steam_id| {
            let service = self.clone();
            async move {
                let value = service.fetch_remote(&steam_id).await;
                service.store_cache_entry(&steam_id, value.clone()).await;
                (steam_id, value)
            }
        }))
        .buffer_unordered(self.concurrency_limit)
        .collect::<Vec<_>>()
        .await;

        for (steam_id, value) in fetched {
            results.insert(steam_id, value);
        }

        results
    }

    async fn store_cache_entry(&self, steam_id: &str, value: Option<Value>) {
        let mut cache = self.cache.write().await;
        cache.insert(
            steam_id.to_string(),
            CacheEntry {
                value,
                expires_at: Instant::now() + self.ttl,
            },
        );
    }

    async fn fetch_remote(&self, steam_id: &str) -> Option<Value> {
        let url = format!("https://api.gokz.top/api/v1/bans?steamid64={}", steam_id);

        match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => match resp.json::<Value>().await {
                Ok(data) => Some(data),
                Err(e) => {
                    tracing::error!(
                        "Failed to parse cached GOKZ ban response for {}: {}",
                        steam_id,
                        e
                    );
                    None
                }
            },
            Ok(resp) => {
                tracing::warn!(
                    "GOKZ ban API returned {} for steam_id {}",
                    resp.status(),
                    steam_id
                );
                None
            }
            Err(e) => {
                tracing::error!("Failed to call cached GOKZ ban API for {}: {}", steam_id, e);
                None
            }
        }
    }
}
