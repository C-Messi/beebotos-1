//! Unified storage backend for BeeWeb Update Server
//!
//! Supports both in-memory and SQLite persistent storage.

use crate::models::{ReleaseRecord, SemVer, UpdateMetricRecord, UpdateStatus, VersionInfo};
use chrono::Utc;
use dashmap::DashMap;
use serde::Serialize;
use std::sync::Arc;

/// Storage backend for releases and metrics
#[derive(Debug, Clone)]
pub struct Storage {
    releases: Arc<DashMap<String, Vec<ReleaseRecord>>>,
    metrics: Arc<DashMap<String, UpdateMetricRecord>>,
    db: Option<crate::db::DbStorage>,
}

impl Storage {
    pub fn new() -> Self {
        Self {
            releases: Arc::new(DashMap::new()),
            metrics: Arc::new(DashMap::new()),
            db: None,
        }
    }

    pub fn with_db(db: crate::db::DbStorage) -> Self {
        Self {
            releases: Arc::new(DashMap::new()),
            metrics: Arc::new(DashMap::new()),
            db: Some(db),
        }
    }

    /// Seed with sample data for testing
    pub async fn seed_sample_data(&self) {
        use crate::models::{PackageInfo, PackageType, Platform, UpdateMetadata, UpdatePriority};
        use std::collections::HashMap;

        let mut release_notes = HashMap::new();
        release_notes.insert(
            "en".to_string(),
            "Bug fixes and performance improvements".to_string(),
        );
        release_notes.insert(
            "zh".to_string(),
            "修复漏洞并提升性能".to_string(),
        );

        // Gateway release
        let gateway_v110 = VersionInfo {
            version: SemVer {
                major: 1,
                minor: 1,
                patch: 0,
                pre: None,
                build: None,
            },
            released_at: Utc::now(),
            mandatory: false,
            min_supported_version: Some(SemVer {
                major: 1,
                minor: 0,
                patch: 0,
                pre: None,
                build: None,
            }),
            priority: UpdatePriority::High,
            release_notes: release_notes.clone(),
            packages: vec![
                PackageInfo {
                    id: "gateway-1.1.0-linux-amd64".to_string(),
                    platform: Platform::Linux,
                    package_type: PackageType::Full,
                    download_url: "/api/v1/updates/download/gateway-1.1.0-linux-amd64".to_string(),
                    hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
                    size: 15_728_640,
                    signature: "dummy_signature_for_demo".to_string(),
                    base_version: None,
                },
                PackageInfo {
                    id: "gateway-1.1.0-windows-amd64".to_string(),
                    platform: Platform::Windows,
                    package_type: PackageType::Full,
                    download_url: "/api/v1/updates/download/gateway-1.1.0-windows-amd64".to_string(),
                    hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
                    size: 16_384_000,
                    signature: "dummy_signature_for_demo".to_string(),
                    base_version: None,
                },
            ],
            metadata: UpdateMetadata {
                min_supported_version: Some("1.0.0".to_string()),
                deprecated_versions: None,
                rollout_percentage: Some(100),
            },
        };

        let gateway_release = ReleaseRecord {
            app_name: "gateway".to_string(),
            version: gateway_v110.version.clone(),
            channel: "stable".to_string(),
            version_info: gateway_v110,
            packages_dir: "/data/packages/gateway/1.1.0".to_string(),
            created_at: Utc::now(),
        };

        self.releases
            .insert("gateway".to_string(), vec![gateway_release]);

        // CLI release
        let cli_v110 = VersionInfo {
            version: SemVer {
                major: 1,
                minor: 1,
                patch: 0,
                pre: None,
                build: None,
            },
            released_at: Utc::now(),
            mandatory: false,
            min_supported_version: Some(SemVer {
                major: 1,
                minor: 0,
                patch: 0,
                pre: None,
                build: None,
            }),
            priority: UpdatePriority::Medium,
            release_notes: release_notes.clone(),
            packages: vec![
                PackageInfo {
                    id: "cli-1.1.0-linux-amd64".to_string(),
                    platform: Platform::Linux,
                    package_type: PackageType::Full,
                    download_url: "/api/v1/updates/download/cli-1.1.0-linux-amd64".to_string(),
                    hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
                    size: 8_192_000,
                    signature: "dummy_signature_for_demo".to_string(),
                    base_version: None,
                },
                PackageInfo {
                    id: "cli-1.1.0-macos-amd64".to_string(),
                    platform: Platform::MacOS,
                    package_type: PackageType::Full,
                    download_url: "/api/v1/updates/download/cli-1.1.0-macos-amd64".to_string(),
                    hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
                    size: 8_388_608,
                    signature: "dummy_signature_for_demo".to_string(),
                    base_version: None,
                },
            ],
            metadata: UpdateMetadata {
                min_supported_version: Some("1.0.0".to_string()),
                deprecated_versions: None,
                rollout_percentage: Some(100),
            },
        };

        let cli_release = ReleaseRecord {
            app_name: "cli".to_string(),
            version: cli_v110.version.clone(),
            channel: "stable".to_string(),
            version_info: cli_v110,
            packages_dir: "/data/packages/cli/1.1.0".to_string(),
            created_at: Utc::now(),
        };

        self.releases.insert("cli".to_string(), vec![cli_release]);

        // Web release
        let web_v110 = VersionInfo {
            version: SemVer {
                major: 1,
                minor: 1,
                patch: 0,
                pre: None,
                build: None,
            },
            released_at: Utc::now(),
            mandatory: true,
            min_supported_version: Some(SemVer {
                major: 1,
                minor: 0,
                patch: 0,
                pre: None,
                build: None,
            }),
            priority: UpdatePriority::Critical,
            release_notes,
            packages: vec![PackageInfo {
                id: "web-1.1.0-wasm".to_string(),
                platform: Platform::Wasm,
                package_type: PackageType::Full,
                download_url: "/api/v1/updates/download/web-1.1.0-wasm".to_string(),
                hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
                size: 4_194_304,
                signature: "dummy_signature_for_demo".to_string(),
                base_version: None,
            }],
            metadata: UpdateMetadata {
                min_supported_version: Some("1.0.0".to_string()),
                deprecated_versions: None,
                rollout_percentage: Some(100),
            },
        };

        let web_release = ReleaseRecord {
            app_name: "web".to_string(),
            version: web_v110.version.clone(),
            channel: "stable".to_string(),
            version_info: web_v110,
            packages_dir: "/data/packages/web/1.1.0".to_string(),
            created_at: Utc::now(),
        };

        self.releases.insert("web".to_string(), vec![web_release]);
    }

    /// Find the latest release for an app on a given channel
    pub async fn find_latest_release(
        &self,
        app_name: &str,
        channel: &str,
    ) -> Option<ReleaseRecord> {
        // Prefer database if available
        if let Some(ref db) = self.db {
            return db.find_latest_release(app_name, channel).await.ok().flatten();
        }
        self.releases
            .get(app_name)?
            .iter()
            .filter(|r| r.channel == channel)
            .max_by_key(|r| r.version.clone())
            .cloned()
    }

    /// Find a specific package by ID across all releases
    pub async fn find_package(&self, package_id: &str) -> Option<(ReleaseRecord, crate::models::PackageInfo)> {
        if let Some(ref db) = self.db {
            return db.find_package(package_id).await.ok().flatten();
        }
        for entry in self.releases.iter() {
            for release in entry.value() {
                if let Some(pkg) = release.version_info.packages.iter().find(|p| p.id == package_id) {
                    return Some((release.clone(), pkg.clone()));
                }
            }
        }
        None
    }

    /// Save an update report metric
    pub async fn save_metric(&self, metric: UpdateMetricRecord) {
        if let Some(ref db) = self.db {
            let _ = db.save_metric(&metric).await;
        }
        self.metrics.insert(metric.id.clone(), metric);
    }

    /// Get all metrics (for admin/debug)
    pub async fn get_metrics(&self) -> Vec<UpdateMetricRecord> {
        if let Some(ref db) = self.db {
            return db.get_metrics().await.unwrap_or_default();
        }
        self.metrics
            .iter()
            .map(|e| e.value().clone())
            .collect()
    }

    /// Get metrics summary by status
    pub async fn get_metrics_summary(&self) -> MetricsSummary {
        if let Some(ref db) = self.db {
            return db.get_metrics_summary().await.unwrap_or_default();
        }
        let mut summary = MetricsSummary::default();
        for entry in self.metrics.iter() {
            match entry.value().status {
                UpdateStatus::Completed => summary.success_count += 1,
                UpdateStatus::Failed => summary.failure_count += 1,
                UpdateStatus::RolledBack => summary.rollback_count += 1,
                _ => summary.in_progress_count += 1,
            }
        }
        summary
    }

    /// Add a new release
    pub fn add_release(&self, record: ReleaseRecord) {
        let mut entry = self.releases.entry(record.app_name.clone()).or_default();
        entry.push(record);
    }
}

impl Default for Storage {
    fn default() -> Self {
        Self::new()
    }
}

/// Metrics summary
#[derive(Debug, Clone, Default, Serialize)]
pub struct MetricsSummary {
    pub total_count: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub rollback_count: u64,
    pub in_progress_count: u64,
}
