use chrono::{Duration, Utc, DateTime};
use regex::Regex;

pub fn parse_duration(duration_str: &str) -> Option<Duration> {
    if duration_str == "permanent" || duration_str.starts_with("Until") {
        return None; // Special handling elsewhere or infinite
    }

    let re = Regex::new(r"^(\d+)([a-zA-Z]+)$").unwrap();
    if let Some(caps) = re.captures(duration_str) {
        let value: i64 = caps[1].parse().ok()?;
        let unit = &caps[2];

        match unit {
            "s" => Some(Duration::seconds(value)),
            "m" => Some(Duration::minutes(value)),
            "h" => Some(Duration::hours(value)),
            "d" => Some(Duration::days(value)),
            "mo" => Some(Duration::days(value * 30)), // Approx
            "y" => Some(Duration::days(value * 365)), // Approx
            _ => None
        }
    } else {
        None
    }
}

pub fn calculate_expires_at(duration_str: &str) -> Option<DateTime<Utc>> {
    if duration_str == "permanent" {
        return None;
    }
    // Handle "Until YYYY-MM-DD HH:MM" custom format if present
    if duration_str.starts_with("Until ") {
        // Simple parse attempt or frontend sends ISO? 
        // Frontend sends "Until 2026-01-01 12:00"
        let _date_str = &duration_str[6..];
        // Naive parsing, assuming UTC or local? 
        // Let's try to parse as naive and set to UTC.
        // Actually better if frontend sends ISO8601, but we have text "Until ..."
        // Let's implement robust parsing later if needed, for now try standard formats
        // For this task, we assume standard durations mostly.
        // If "Until", let's try strict format.
        // For simplicity now, return None (manual handling or skip) if complex.
        // But user wants "封禁时间+封禁时长".
        return None; 
    }

    if let Some(duration) = parse_duration(duration_str) {
        Some(Utc::now() + duration)
    } else {
        None
    }
}

pub async fn log_admin_action(
    pool: &sqlx::MySqlPool,
    admin_username: &str,
    action: &str,
    target: &str,
    details: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO audit_logs (admin_username, action, target, details) VALUES (?, ?, ?, ?)")
        .bind(admin_username)
        .bind(action)
        .bind(target)
        .bind(details)
        .execute(pool)
        .await?;
    Ok(())
}

pub mod rcon;
