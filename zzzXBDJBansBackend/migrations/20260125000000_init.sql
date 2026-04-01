CREATE TABLE IF NOT EXISTS admins (
    id BIGSERIAL PRIMARY KEY,
    username VARCHAR(64) NOT NULL UNIQUE,
    password VARCHAR(256) NOT NULL,
    role VARCHAR(32) NOT NULL DEFAULT 'admin',
    steam_id VARCHAR(32),
    steam_id_3 VARCHAR(64),
    steam_id_64 VARCHAR(64),
    remark TEXT,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS bans (
    id BIGSERIAL PRIMARY KEY,
    name VARCHAR(128) NOT NULL,
    steam_id VARCHAR(32) NOT NULL,
    steam_id_3 VARCHAR(64),
    steam_id_64 VARCHAR(64),
    ip VARCHAR(45) NOT NULL,
    ban_type VARCHAR(32) NOT NULL,
    reason TEXT,
    duration VARCHAR(32) NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'active',
    admin_name VARCHAR(64),
    expires_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    server_id BIGINT NULL
);

CREATE TABLE IF NOT EXISTS audit_logs (
    id BIGSERIAL PRIMARY KEY,
    admin_username VARCHAR(64) NOT NULL,
    action VARCHAR(64) NOT NULL,
    target VARCHAR(128),
    details TEXT,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS player_records (
    id BIGSERIAL PRIMARY KEY,
    player_name VARCHAR(128) NOT NULL,
    steam_id VARCHAR(32) NOT NULL,
    player_ip VARCHAR(45) NOT NULL,
    server_name VARCHAR(128),
    server_address VARCHAR(64),
    connect_time TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);
