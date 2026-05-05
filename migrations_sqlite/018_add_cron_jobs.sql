-- Cron Jobs tables for BeeBotOS Scheduler
-- Provides persistent task scheduling with execution history

CREATE TABLE cron_jobs (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    name TEXT NOT NULL,
    description TEXT DEFAULT '',
    schedule_type TEXT NOT NULL CHECK(schedule_type IN ('at', 'every', 'cron')),
    schedule_expr TEXT NOT NULL,
    timezone TEXT DEFAULT 'UTC',
    prompt TEXT NOT NULL,
    enabled INTEGER DEFAULT 1,
    context_mode TEXT DEFAULT 'isolated' CHECK(context_mode IN ('main', 'isolated')),
    delivery_channel TEXT DEFAULT '',
    delivery_target TEXT DEFAULT '',
    max_runs INTEGER DEFAULT NULL,
    run_count INTEGER DEFAULT 0,
    last_run_at TEXT DEFAULT NULL,
    next_run_at TEXT DEFAULT NULL,
    created_by TEXT DEFAULT '',
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX idx_cron_jobs_enabled ON cron_jobs(enabled);
CREATE INDEX idx_cron_jobs_next_run ON cron_jobs(next_run_at) WHERE enabled = 1;

CREATE TABLE cron_job_runs (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    job_id TEXT NOT NULL REFERENCES cron_jobs(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK(status IN ('pending', 'running', 'success', 'failed', 'cancelled')),
    output TEXT DEFAULT '',
    error TEXT DEFAULT '',
    started_at TEXT DEFAULT (datetime('now')),
    completed_at TEXT DEFAULT NULL,
    triggered_by TEXT DEFAULT 'scheduler'
);

CREATE INDEX idx_cron_job_runs_job ON cron_job_runs(job_id);
CREATE INDEX idx_cron_job_runs_status ON cron_job_runs(status);
