-- Add migration script here
-- 1. Admins
CREATE TABLE IF NOT EXISTS admins (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    username VARCHAR(64) NOT NULL UNIQUE,
    password VARCHAR(256) NOT NULL,
    role ENUM('super_admin', 'admin') NOT NULL DEFAULT 'admin',
    steam_id VARCHAR(32),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- 2. Bans
CREATE TABLE IF NOT EXISTS bans (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(128) NOT NULL,
    steam_id VARCHAR(32) NOT NULL,
    ip VARCHAR(45) NOT NULL,
    ban_type ENUM('account', 'ip') NOT NULL,
    reason TEXT,
    duration VARCHAR(32) NOT NULL,
    status ENUM('active', 'unbanned', 'expired') NOT NULL DEFAULT 'active',
    admin_name VARCHAR(64),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- 3. Audit Logs
CREATE TABLE IF NOT EXISTS audit_logs (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    admin_username VARCHAR(64) NOT NULL,
    action VARCHAR(64) NOT NULL,
    target VARCHAR(128),
    details TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- 4. Player Records
CREATE TABLE IF NOT EXISTS player_records (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    player_name VARCHAR(128) NOT NULL,
    steam_id VARCHAR(32) NOT NULL,
    player_ip VARCHAR(45) NOT NULL,
    server_name VARCHAR(128),
    server_address VARCHAR(64),
    connect_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Initial Super Admin (admin/password) - BCrypt hash needed ideally, but for start maybe plain or pre-hashed?
-- 'password' hashed with bcrypt cost 4 is '$2a$04$...' but let's assume the app handles hashing.
-- For now I will insert a placeholder row if empty.
-- Default admin will be created by application logic if table is empty
