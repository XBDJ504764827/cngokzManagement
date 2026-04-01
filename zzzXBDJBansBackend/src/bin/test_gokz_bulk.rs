use reqwest::Client;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    
    // GOKZ API URL - Testing comma separated IDs
    // Test IPs: 76561198031336449 (some id), 76561197960287930 (GabeN)
    let steam_ids = "76561198114674986,76561198031336449";
    let url = format!("https://api.gokz.top/api/v1/bans?steamid64={}", steam_ids);

    println!("Querying: {}", url);

    let response = client.get(&url).send().await?;
    
    if response.status().is_success() {
        let json: Value = response.json().await?;
        println!("Response: {:#?}", json);
    } else {
        println!("Request failed: {}", response.status());
    }

    Ok(())
}
