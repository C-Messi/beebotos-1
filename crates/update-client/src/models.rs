//! Shared data models for update client

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Semantic version wrapper
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
}

impl std::fmt::Display for SemVer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = &self.pre {
            write!(f, "-{}", pre)?;
        }
        if let Some(build) = &self.build {
            write!(f, "+{}", build)?;
        }
        Ok(())
    }
}

impl From<semver::Version> for SemVer {
    fn from(v: semver::Version) -> Self {
        Self {
            major: v.major,
            minor: v.minor,
            patch: v.patch,
            pre: if v.pre.is_empty() { None } else { Some(v.pre.to_string()) },
            build: if v.build.is_empty() { None } else { Some(v.build.to_string()) },
        }
    }
}

impl TryFrom<&str> for SemVer {
    type Error = semver::Error;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Ok(semver::Version::parse(s)?.into())
    }
}

/// Target platform
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Windows,
    Linux,
    MacOS,
    Wasm,
}

/// Package type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageType {
    Full,
    Delta,
    Patch,
}

/// Update priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePriority {
    Critical,
    High,
    Medium,
    Low,
}

/// Update status
#[derive(Debug, Clone, Default, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    Downloading,
    Verifying,
    Installing,
    Restarting,
    Completed,
    Failed,
    RolledBack,
}

/// Package information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub id: String,
    pub platform: Platform,
    pub package_type: PackageType,
    pub download_url: String,
    pub hash: String,
    pub size: u64,
    pub signature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_version: Option<SemVer>,
}

/// Update metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_supported_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated_versions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollout_percentage: Option<u8>,
}

/// Version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: SemVer,
    pub released_at: DateTime<Utc>,
    pub mandatory: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_supported_version: Option<SemVer>,
    pub priority: UpdatePriority,
    pub release_notes: HashMap<String, String>,
    pub packages: Vec<PackageInfo>,
    pub metadata: UpdateMetadata,
}

/// Update state
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateState {
    pub status: UpdateStatus,
    pub current_version: SemVer,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_version: Option<SemVer>,
    pub download_progress: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub retry_count: u32,
}

/// Update check request
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateCheckRequest {
    pub app_name: String,
    pub current_version: String,
    pub platform: String,
    #[serde(default = "default_channel")]
    pub channel: String,
}

fn default_channel() -> String {
    "stable".to_string()
}

/// Update check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResponse {
    pub has_update: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_info: Option<VersionInfo>,
}

/// Update report request
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateReportRequest {
    pub app_name: String,
    pub device_id: String,
    pub current_version: String,
    pub target_version: String,
    pub status: UpdateStatus,
    #[serde(default)]
    pub duration_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Signature data format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureData {
    pub version: u32,
    pub algorithm: String,
    pub public_key: String,
    pub signature: String,
}
