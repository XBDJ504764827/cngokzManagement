-- Add migration script here
CREATE TABLE IF NOT EXISTS whitelist (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    steam_id VARCHAR(32) NOT NULL UNIQUE,
    name VARCHAR(128) NOT NULL,
    status ENUM('approved', 'pending', 'rejected') NOT NULL DEFAULT 'approved',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
