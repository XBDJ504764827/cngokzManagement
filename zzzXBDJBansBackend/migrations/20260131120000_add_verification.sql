CREATE TABLE IF NOT EXISTS player_verifications (
    steam_id VARCHAR(32) NOT NULL PRIMARY KEY,
    status ENUM('pending', 'allowed', 'denied') NOT NULL DEFAULT 'pending',
    reason TEXT NULL,
    steam_level INT NULL,
    playtime_minutes INT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    INDEX idx_status (status)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
