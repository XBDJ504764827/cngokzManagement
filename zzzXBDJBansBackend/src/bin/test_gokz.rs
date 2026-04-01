use reqwest;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let steam_id = "76561199260261806"; // Need a real banned ID to be sure, but let's test connectivity first.
    // Try a known banned ID if possible. 
    // I'll try to find one online or just use a random one to see if I get an empty list or error.
    
    let url = format!("https://api.gokz.top/api/v1/bans?steamid64={}", steam_id);
    println!("Fetching {}", url);

    let client = reqwest::Client::new();
    let resp = client.get(&url)
        .send()
        .await?;

    println!("Status: {}", resp.status());
    let text = resp.text().await?;
    println!("Body: {}", text);

    Ok(())
}
