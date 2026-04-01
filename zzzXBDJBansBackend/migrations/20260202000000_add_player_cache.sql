CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TABLE IF NOT EXISTS player_cache (
    steam_id VARCHAR(32) NOT NULL PRIMARY KEY,
    player_name VARCHAR(128) NULL,
    ip_address VARCHAR(45) NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    reason TEXT NULL,
    steam_level INT NULL,
    playtime_minutes INT NULL,
    gokz_rating NUMERIC(10,2) NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_player_cache_status ON player_cache (status);

DROP TRIGGER IF EXISTS trg_player_cache_updated_at ON player_cache;
CREATE TRIGGER trg_player_cache_updated_at
    BEFORE UPDATE ON player_cache
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();
