-- 🆕 FIX (P0): Add composite index for cron_jobs pending query
-- Addresses slow query warning: SELECT ... FROM cron_jobs 
-- WHERE enabled = 1 AND schedule_type = 'at' AND next_run_at <= ?1 
-- ORDER BY next_run_at ASC
-- Previous indexes:
--   - idx_cron_jobs_enabled (single column on enabled)
--   - idx_cron_jobs_next_run (partial index WHERE enabled = 1)
-- Neither covers the full WHERE clause with schedule_type filter.

CREATE INDEX IF NOT EXISTS idx_cron_jobs_enabled_schedule_next 
ON cron_jobs(enabled, schedule_type, next_run_at);
