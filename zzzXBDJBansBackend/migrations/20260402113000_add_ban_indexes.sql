CREATE INDEX IF NOT EXISTS idx_bans_created_at
    ON bans(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_bans_status_expires_at
    ON bans(status, expires_at);

CREATE INDEX IF NOT EXISTS idx_bans_steam_id
    ON bans(steam_id);

CREATE INDEX IF NOT EXISTS idx_bans_steam_id_64
    ON bans(steam_id_64);

CREATE INDEX IF NOT EXISTS idx_bans_ip
    ON bans(ip);
