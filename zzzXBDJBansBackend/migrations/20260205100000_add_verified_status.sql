-- 修改 status 枚举，添加 verified 状态
-- verified 表示后端已获取数据，由插件判断是否放行

ALTER TABLE player_cache 
    MODIFY COLUMN status ENUM('pending', 'verified', 'allowed', 'denied') 
    NOT NULL DEFAULT 'pending';
