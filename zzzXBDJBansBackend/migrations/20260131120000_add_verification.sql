CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TABLE IF NOT EXISTS player_verifications (
    steam_id VARCHAR(32) NOT NULL PRIMARY KEY,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    reason TEXT NULL,
    steam_level INT NULL,
    playtime_minutes INT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_player_verifications_status ON player_verifications (status);

DROP TRIGGER IF EXISTS trg_player_verifications_updated_at ON player_verifications;
CREATE TRIGGER trg_player_verifications_updated_at
    BEFORE UPDATE ON player_verifications
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();
