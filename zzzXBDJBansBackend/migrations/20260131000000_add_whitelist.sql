CREATE TABLE IF NOT EXISTS whitelist (
    id BIGSERIAL PRIMARY KEY,
    steam_id VARCHAR(32) NOT NULL UNIQUE,
    steam_id_3 VARCHAR(64),
    steam_id_64 VARCHAR(64),
    name VARCHAR(128) NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'approved',
    reject_reason TEXT NULL,
    admin_name VARCHAR(255) NULL,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);
