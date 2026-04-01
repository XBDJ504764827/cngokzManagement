-- Add expires_at column to bans table if it doesn't exist
-- Using PREPARE statement for compatibility with older MySQL/MariaDB that lack IF NOT EXISTS for columns

SET @dbname = DATABASE();
SET @tablename = "bans";
SET @columnname = "expires_at";
SET @preparedStatement = (SELECT IF(
  (
    SELECT COUNT(*) FROM INFORMATION_SCHEMA.COLUMNS
    WHERE
      (table_name = @tablename)
      AND (table_schema = @dbname)
      AND (column_name = @columnname)
  ) > 0,
  "SELECT 1",
  "ALTER TABLE bans ADD COLUMN expires_at TIMESTAMP NULL;"
));
PREPARE alterIfNotExists FROM @preparedStatement;
EXECUTE alterIfNotExists;
DEALLOCATE PREPARE alterIfNotExists;
