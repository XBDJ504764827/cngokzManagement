-- Remove all denied records from cache and verification tables
DELETE FROM player_cache WHERE status = 'denied';
DELETE FROM player_verifications WHERE status = 'denied';
