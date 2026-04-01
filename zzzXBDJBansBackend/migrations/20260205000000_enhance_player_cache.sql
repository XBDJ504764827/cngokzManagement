-- 添加玩家名称、IP地址、GOKZ Rating 字段到 player_cache 表
-- 使用兼容旧版 MySQL 的语法

-- 添加 player_name 字段
ALTER TABLE player_cache ADD COLUMN player_name VARCHAR(128) NULL AFTER steam_id;

-- 添加 ip_address 字段  
ALTER TABLE player_cache ADD COLUMN ip_address VARCHAR(45) NULL AFTER player_name;

-- 添加 gokz_rating 字段
ALTER TABLE player_cache ADD COLUMN gokz_rating DECIMAL(10,2) NULL AFTER playtime_minutes;
