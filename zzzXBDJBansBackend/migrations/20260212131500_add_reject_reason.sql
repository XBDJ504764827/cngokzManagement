ALTER TABLE whitelist
    ADD COLUMN IF NOT EXISTS reject_reason TEXT NULL;
