-- Set cron job timezone defaults to China Beijing time.
-- SQLite cannot alter a column default in place without rebuilding the table,
-- so existing UTC/empty rows are normalized here; application code now applies
-- Asia/Shanghai as the default for new rows.

UPDATE cron_jobs
SET timezone = 'Asia/Shanghai'
WHERE timezone IS NULL OR timezone = '' OR timezone = 'UTC';
