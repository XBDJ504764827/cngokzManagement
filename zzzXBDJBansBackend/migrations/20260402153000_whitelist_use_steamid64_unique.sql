DELETE FROM whitelist a
USING whitelist b
WHERE a.id < b.id
  AND a.steam_id_64 IS NOT NULL
  AND a.steam_id_64 = b.steam_id_64;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_name = 'whitelist'
          AND constraint_name = 'whitelist_steam_id_key'
    ) THEN
        ALTER TABLE whitelist
            DROP CONSTRAINT whitelist_steam_id_key;
    END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_whitelist_steam_id_64_unique
    ON whitelist (steam_id_64)
    WHERE steam_id_64 IS NOT NULL;
