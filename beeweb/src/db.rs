//! SQLite persistent storage for BeeWeb Update Server

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::sqlite::SqlitePool;
use sqlx::Row;

use crate::models::{
    PackageInfo, Platform, ReleaseRecord, SemVer, UpdateMetadata, UpdateMetricRecord,
    UpdatePriority, UpdateStatus, VersionInfo,
};

/// Database storage backend
#[derive(Clone, Debug)]
pub struct DbStorage {
    pool: SqlitePool,
}

impl DbStorage {
    pub async fn new(database_url: &str) -> anyhow::Result<Self> {
        let pool = SqlitePool::connect(database_url).await?;
        Self::migrate(&pool).await?;
        Ok(Self { pool })
    }

    async fn migrate(pool: &SqlitePool) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS releases (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                app_name TEXT NOT NULL,
                version TEXT NOT NULL,
                channel TEXT NOT NULL DEFAULT 'stable',
                released_at TEXT NOT NULL,
                mandatory INTEGER NOT NULL DEFAULT 0,
                min_supported_version TEXT,
                priority TEXT NOT NULL DEFAULT 'medium',
                release_notes TEXT,
                metadata TEXT,
                packages_dir TEXT,
                created_at TEXT NOT NULL,
                UNIQUE(app_name, version, channel)
            );

            CREATE TABLE IF NOT EXISTS packages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                release_id INTEGER NOT NULL,
                package_id TEXT NOT NULL UNIQUE,
                platform TEXT NOT NULL,
                package_type TEXT NOT NULL DEFAULT 'full',
                download_url TEXT NOT NULL,
                hash TEXT NOT NULL,
                size INTEGER NOT NULL,
                signature TEXT NOT NULL,
                base_version TEXT,
                FOREIGN KEY(release_id) REFERENCES releases(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS metrics (
                id TEXT PRIMARY KEY,
                app_name TEXT NOT NULL,
                device_id TEXT NOT NULL,
                current_version TEXT NOT NULL,
                target_version TEXT NOT NULL,
                status TEXT NOT NULL,
                duration_secs INTEGER NOT NULL DEFAULT 0,
                error TEXT,
                reported_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_releases_app_channel ON releases(app_name, channel);
            CREATE INDEX IF NOT EXISTS idx_metrics_app ON metrics(app_name);
            "#,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Save a release with its packages
    pub async fn save_release(&self, record: &ReleaseRecord) -> anyhow::Result<i64> {
        let release_notes_json =
            serde_json::to_string(&record.version_info.release_notes).unwrap_or_default();
        let metadata_json =
            serde_json::to_string(&record.version_info.metadata).unwrap_or_default();
        let mandatory = if record.version_info.mandatory { 1 } else { 0 };
        let min_sv = record
            .version_info
            .min_supported_version
            .as_ref()
            .map(|v| v.to_string());

        let result = sqlx::query(
            r#"
            INSERT OR REPLACE INTO releases
            (app_name, version, channel, released_at, mandatory, min_supported_version, priority, release_notes, metadata, packages_dir, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
        )
        .bind(&record.app_name)
        .bind(&record.version.to_string())
        .bind(&record.channel)
        .bind(&record.version_info.released_at.to_rfc3339())
        .bind(mandatory)
        .bind(min_sv)
        .bind(format!("{:?}", record.version_info.priority).to_lowercase())
        .bind(&release_notes_json)
        .bind(&metadata_json)
        .bind(&record.packages_dir)
        .bind(&record.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        let release_id = result.last_insert_rowid();

        // Save packages
        for pkg in &record.version_info.packages {
            sqlx::query(
                r#"
                INSERT OR REPLACE INTO packages
                (release_id, package_id, platform, package_type, download_url, hash, size, signature, base_version)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
            )
            .bind(release_id)
            .bind(&pkg.id)
            .bind(format!("{:?}", pkg.platform).to_lowercase())
            .bind(format!("{:?}", pkg.package_type).to_lowercase())
            .bind(&pkg.download_url)
            .bind(&pkg.hash)
            .bind(pkg.size as i64)
            .bind(&pkg.signature)
            .bind(pkg.base_version.as_ref().map(|v| v.to_string()))
            .execute(&self.pool)
            .await?;
        }

        Ok(release_id)
    }

    /// Find the latest release for an app on a given channel
    pub async fn find_latest_release(
        &self,
        app_name: &str,
        channel: &str,
    ) -> anyhow::Result<Option<ReleaseRecord>> {
        let row = sqlx::query(
            r#"
            SELECT id, app_name, version, channel, released_at, mandatory,
                   min_supported_version, priority, release_notes, metadata,
                   packages_dir, created_at
            FROM releases
            WHERE app_name = ?1 AND channel = ?2
            ORDER BY version DESC
            LIMIT 1
            "#,
        )
        .bind(app_name)
        .bind(channel)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => {
                let release_id: i64 = row.get("id");
                let record = self.row_to_release_record(&row, release_id).await?;
                Ok(Some(record))
            }
            None => Ok(None),
        }
    }

    /// Find a specific package by ID
    pub async fn find_package(
        &self,
        package_id: &str,
    ) -> anyhow::Result<Option<(ReleaseRecord, PackageInfo)>> {
        let pkg_row = sqlx::query(
            r#"
            SELECT p.*, r.id as release_id
            FROM packages p
            JOIN releases r ON p.release_id = r.id
            WHERE p.package_id = ?1
            LIMIT 1
            "#,
        )
        .bind(package_id)
        .fetch_optional(&self.pool)
        .await?;

        match pkg_row {
            Some(row) => {
                let release_id: i64 = row.get("release_id");
                let release_row = sqlx::query(
                    r#"
                    SELECT * FROM releases WHERE id = ?1
                    "#,
                )
                .bind(release_id)
                .fetch_one(&self.pool)
                .await?;

                let release = self.row_to_release_record(&release_row, release_id).await?;
                let package = self.row_to_package_info(&row)?;
                Ok(Some((release, package)))
            }
            None => Ok(None),
        }
    }

    /// Save an update report metric
    pub async fn save_metric(&self, metric: &UpdateMetricRecord) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO metrics
            (id, app_name, device_id, current_version, target_version, status, duration_secs, error, reported_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
        )
        .bind(&metric.id)
        .bind(&metric.app_name)
        .bind(&metric.device_id)
        .bind(&metric.current_version)
        .bind(&metric.target_version)
        .bind(format!("{:?}", metric.status).to_lowercase())
        .bind(metric.duration_secs as i64)
        .bind(&metric.error)
        .bind(&metric.reported_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get metrics summary by status
    pub async fn get_metrics_summary(&self) -> anyhow::Result<crate::storage::MetricsSummary> {
        let rows = sqlx::query(
            r#"
            SELECT status, COUNT(*) as cnt FROM metrics GROUP BY status
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut summary = crate::storage::MetricsSummary::default();
        for row in rows {
            let status: String = row.get("status");
            let cnt: i64 = row.get("cnt");
            match status.as_str() {
                "completed" => summary.success_count = cnt as u64,
                "failed" => summary.failure_count = cnt as u64,
                "rolledback" => summary.rollback_count = cnt as u64,
                _ => summary.in_progress_count += cnt as u64,
            }
        }
        summary.total_count = summary.success_count
            + summary.failure_count
            + summary.rollback_count
            + summary.in_progress_count;
        Ok(summary)
    }

    /// Get all metrics
    pub async fn get_metrics(&self) -> anyhow::Result<Vec<UpdateMetricRecord>> {
        let rows = sqlx::query("SELECT * FROM metrics ORDER BY reported_at DESC LIMIT 1000")
            .fetch_all(&self.pool)
            .await?;

        let mut metrics = Vec::new();
        for row in rows {
            metrics.push(self.row_to_metric_record(&row)?);
        }
        Ok(metrics)
    }

    async fn row_to_release_record(
        &self,
        row: &sqlx::sqlite::SqliteRow,
        release_id: i64,
    ) -> anyhow::Result<ReleaseRecord> {
        let app_name: String = row.get("app_name");
        let version_str: String = row.get("version");
        let version = SemVer::try_from(version_str.as_str()).unwrap_or_else(|_| SemVer {
            major: 0,
            minor: 0,
            patch: 0,
            pre: None,
            build: None,
        });
        let channel: String = row.get("channel");
        let released_at: String = row.get("released_at");
        let mandatory: i64 = row.get("mandatory");
        let min_supported_version: Option<String> = row.get("min_supported_version");
        let priority_str: String = row.get("priority");
        let release_notes_json: String = row.get("release_notes");
        let metadata_json: String = row.get("metadata");
        let packages_dir: String = row.get("packages_dir");
        let created_at: String = row.get("created_at");

        let released_at = DateTime::parse_from_rfc3339(&released_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let created_at = DateTime::parse_from_rfc3339(&created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let release_notes: HashMap<String, String> =
            serde_json::from_str(&release_notes_json).unwrap_or_default();
        let metadata: UpdateMetadata = serde_json::from_str(&metadata_json).unwrap_or_default();

        let priority = match priority_str.as_str() {
            "critical" => UpdatePriority::Critical,
            "high" => UpdatePriority::High,
            "low" => UpdatePriority::Low,
            _ => UpdatePriority::Medium,
        };

        // Load packages
        let pkg_rows = sqlx::query("SELECT * FROM packages WHERE release_id = ?1")
            .bind(release_id)
            .fetch_all(&self.pool)
            .await?;

        let mut packages = Vec::new();
        for pkg_row in pkg_rows {
            packages.push(self.row_to_package_info(&pkg_row)?);
        }

        let version_info = VersionInfo {
            version: version.clone(),
            released_at,
            mandatory: mandatory != 0,
            min_supported_version: min_supported_version
                .and_then(|v| SemVer::try_from(v.as_str()).ok()),
            priority,
            release_notes,
            packages,
            metadata,
        };

        Ok(ReleaseRecord {
            app_name,
            version,
            channel,
            version_info,
            packages_dir,
            created_at,
        })
    }

    fn row_to_package_info(&self, row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<PackageInfo> {
        let package_id: String = row.get("package_id");
        let platform_str: String = row.get("platform");
        let package_type_str: String = row.get("package_type");
        let download_url: String = row.get("download_url");
        let hash: String = row.get("hash");
        let size: i64 = row.get("size");
        let signature: String = row.get("signature");
        let base_version: Option<String> = row.get("base_version");

        let platform = match platform_str.as_str() {
            "windows" => Platform::Windows,
            "macos" => Platform::MacOS,
            "wasm" => Platform::Wasm,
            _ => Platform::Linux,
        };

        let package_type = match package_type_str.as_str() {
            "delta" => crate::models::PackageType::Delta,
            "patch" => crate::models::PackageType::Patch,
            _ => crate::models::PackageType::Full,
        };

        Ok(PackageInfo {
            id: package_id,
            platform,
            package_type,
            download_url,
            hash,
            size: size as u64,
            signature,
            base_version: base_version.and_then(|v| SemVer::try_from(v.as_str()).ok()),
        })
    }

    fn row_to_metric_record(
        &self,
        row: &sqlx::sqlite::SqliteRow,
    ) -> anyhow::Result<UpdateMetricRecord> {
        let id: String = row.get("id");
        let app_name: String = row.get("app_name");
        let device_id: String = row.get("device_id");
        let current_version: String = row.get("current_version");
        let target_version: String = row.get("target_version");
        let status_str: String = row.get("status");
        let duration_secs: i64 = row.get("duration_secs");
        let error: Option<String> = row.get("error");
        let reported_at: String = row.get("reported_at");

        let status = match status_str.as_str() {
            "checking" => UpdateStatus::Checking,
            "downloading" => UpdateStatus::Downloading,
            "verifying" => UpdateStatus::Verifying,
            "installing" => UpdateStatus::Installing,
            "restarting" => UpdateStatus::Restarting,
            "completed" => UpdateStatus::Completed,
            "failed" => UpdateStatus::Failed,
            "rolledback" => UpdateStatus::RolledBack,
            _ => UpdateStatus::Idle,
        };

        let reported_at = DateTime::parse_from_rfc3339(&reported_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        Ok(UpdateMetricRecord {
            id,
            app_name,
            device_id,
            current_version,
            target_version,
            status,
            duration_secs: duration_secs as u64,
            error,
            reported_at,
        })
    }
}
