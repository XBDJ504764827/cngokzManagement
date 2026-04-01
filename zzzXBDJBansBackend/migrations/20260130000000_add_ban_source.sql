ALTER TABLE bans
    ADD COLUMN IF NOT EXISTS server_id BIGINT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_bans_server_id'
    ) THEN
        ALTER TABLE bans
            ADD CONSTRAINT fk_bans_server_id
            FOREIGN KEY (server_id)
            REFERENCES servers(id)
            ON DELETE SET NULL;
    END IF;
END $$;
