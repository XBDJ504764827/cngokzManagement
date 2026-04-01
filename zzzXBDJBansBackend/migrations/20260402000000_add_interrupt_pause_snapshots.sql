CREATE TABLE IF NOT EXISTS interrupt_pause_snapshots (
    id BIGSERIAL PRIMARY KEY,
    server_id BIGINT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    auth_primary VARCHAR(64) NOT NULL,
    auth_steamid64 VARCHAR(64),
    auth_steam3 VARCHAR(64),
    auth_steam2 VARCHAR(64),
    auth_engine VARCHAR(64),
    player_name VARCHAR(128) NOT NULL,
    ip_address VARCHAR(64) NOT NULL,
    map_name VARCHAR(128) NOT NULL,
    mode INTEGER NOT NULL DEFAULT 0,
    course INTEGER NOT NULL DEFAULT 0,
    time_seconds DOUBLE PRECISION NOT NULL DEFAULT 0,
    checkpoint_count INTEGER NOT NULL DEFAULT 0,
    teleport_count INTEGER NOT NULL DEFAULT 0,
    storage_version INTEGER NOT NULL DEFAULT 1,
    payload TEXT NOT NULL,
    restore_status VARCHAR(32) NOT NULL DEFAULT 'none',
    restore_requested_at TIMESTAMPTZ NULL,
    reviewed_at TIMESTAMPTZ NULL,
    reviewed_by VARCHAR(64) NULL,
    reject_reason TEXT NULL,
    restored_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(server_id, auth_primary)
);

CREATE INDEX IF NOT EXISTS idx_interrupt_pause_snapshots_server_id
    ON interrupt_pause_snapshots(server_id);

CREATE INDEX IF NOT EXISTS idx_interrupt_pause_snapshots_auth_steamid64
    ON interrupt_pause_snapshots(auth_steamid64);

CREATE INDEX IF NOT EXISTS idx_interrupt_pause_snapshots_auth_steam3
    ON interrupt_pause_snapshots(auth_steam3);

CREATE INDEX IF NOT EXISTS idx_interrupt_pause_snapshots_auth_steam2
    ON interrupt_pause_snapshots(auth_steam2);

CREATE INDEX IF NOT EXISTS idx_interrupt_pause_snapshots_auth_engine
    ON interrupt_pause_snapshots(auth_engine);

CREATE INDEX IF NOT EXISTS idx_interrupt_pause_snapshots_restore_status
    ON interrupt_pause_snapshots(restore_status);
