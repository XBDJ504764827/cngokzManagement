ALTER TABLE interrupt_pause_snapshots
    DROP CONSTRAINT IF EXISTS interrupt_pause_snapshots_server_id_auth_primary_key;

CREATE UNIQUE INDEX IF NOT EXISTS idx_interrupt_pause_snapshots_active_unique
    ON interrupt_pause_snapshots(server_id, auth_primary)
    WHERE restore_status NOT IN ('restored', 'aborted');

CREATE INDEX IF NOT EXISTS idx_interrupt_pause_snapshots_restored_at
    ON interrupt_pause_snapshots(restored_at);
