//! Process sandbox configuration and management

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};


use crate::script_task::SandboxRequirements;

/// Sandbox configuration for process execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSandboxConfig {
    /// Sandbox identifier
    pub id: String,
    /// Root directory (chroot target)
    pub root_dir: PathBuf,
    /// Bind mounts (host -> guest)
    pub bind_mounts: Vec<BindMount>,
    /// Environment variables
    pub env_vars: HashMap<String, String>,
    /// Resource limits
    pub resource_limits: ResourceLimits,
    /// Network policy
    pub network_policy: NetworkPolicy,
    /// Seccomp profile
    pub seccomp_profile: SeccompProfile,
    /// User/Group IDs to run as
    pub uid: u32,
    pub gid: u32,
}

/// Bind mount configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindMount {
    /// Source path on host
    pub source: PathBuf,
    /// Target path in sandbox
    pub target: PathBuf,
    /// Read-only
    pub read_only: bool,
    /// Create target if missing
    pub create_target: bool,
}

/// Resource limits for sandboxed processes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum memory in bytes
    pub max_memory_bytes: u64,
    /// Maximum CPU time in seconds
    pub max_cpu_time_secs: u64,
    /// Maximum number of processes
    pub max_pids: u32,
    /// Maximum file size in bytes
    pub max_file_size_bytes: u64,
    /// Maximum number of open files
    pub max_open_files: u64,
}

/// Network access policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkPolicy {
    /// No network access
    DenyAll,
    /// Allow outbound to specific domains
    AllowDomains(Vec<String>),
    /// Allow all outbound
    AllowOutbound,
    /// Allow inbound and outbound (restricted use)
    AllowAll,
}

/// Seccomp profile level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeccompProfile {
    /// Minimal syscalls (most restrictive)
    Minimal,
    /// Standard safe syscalls
    Standard,
    /// Permissive (debug only)
    Permissive,
}

impl ProcessSandboxConfig {
    /// Create a restrictive sandbox config from requirements
    pub fn from_requirements(id: impl Into<String>, requirements: &SandboxRequirements, rootfs: PathBuf) -> Self {
        let mut bind_mounts = Vec::new();

        // Add workspace mappings
        for mapping in &requirements.filesystem_paths {
            bind_mounts.push(BindMount {
                source: mapping.host_path.clone(),
                target: mapping.guest_path.clone(),
                read_only: mapping.read_only,
                create_target: true,
            });
        }

        // Always add tmpfs for /tmp
        bind_mounts.push(BindMount {
            source: PathBuf::from("/dev/shm"), // Will be overridden by tmpfs mount
            target: PathBuf::from("/tmp"),
            read_only: false,
            create_target: true,
        });

        Self {
            id: id.into(),
            root_dir: rootfs,
            bind_mounts,
            env_vars: HashMap::new(),
            resource_limits: ResourceLimits {
                max_memory_bytes: requirements.max_memory_mb as u64 * 1024 * 1024,
                max_cpu_time_secs: requirements.max_cpu_time_ms / 1000,
                max_pids: requirements.max_pids,
                max_file_size_bytes: 100 * 1024 * 1024, // 100MB default
                max_open_files: 64,
            },
            network_policy: if requirements.network_allowed {
                if requirements.allowed_domains.is_empty() {
                    NetworkPolicy::AllowOutbound
                } else {
                    NetworkPolicy::AllowDomains(requirements.allowed_domains.clone())
                }
            } else {
                NetworkPolicy::DenyAll
            },
            seccomp_profile: SeccompProfile::Standard,
            uid: 65534, // nobody
            gid: 65534, // nogroup
        }
    }

    /// Generate nsjail configuration protobuf text
    pub fn to_nsjail_config(&self) -> String {
        let mut config = format!(
            r#"mode: ONCE
uidmap {{ inside_id: "{}" outside_id: "{}" count: 1 }}
gidmap {{ inside_id: "{}" outside_id: "{}" count: 1 }}
"#,
            self.uid, self.uid, self.gid, self.gid
        );

        // Resource limits
        config.push_str(&format!(
            "rlimit_as {{ soft: {} hard: {} }}\n",
            self.resource_limits.max_memory_bytes,
            self.resource_limits.max_memory_bytes
        ));
        config.push_str(&format!(
            "rlimit_cpu {{ soft: {} hard: {} }}\n",
            self.resource_limits.max_cpu_time_secs,
            self.resource_limits.max_cpu_time_secs
        ));
        config.push_str(&format!(
            "rlimit_nofile {{ soft: {} hard: {} }}\n",
            self.resource_limits.max_open_files,
            self.resource_limits.max_open_files
        ));

        // Bind mounts
        for mount in &self.bind_mounts {
            config.push_str(&format!(
                "mount {{
  src: \"{}\"
  dst: \"{}\"
  is_bind: true
  rw: {}
}}
",
                mount.source.display(),
                mount.target.display(),
                !mount.read_only
            ));
        }

        // Environment variables
        for (key, value) in &self.env_vars {
            config.push_str(&format!("env {{ name: \"{}\" value: \"{}\" }}\n", key, value));
        }

        // Network policy
        match self.network_policy {
            NetworkPolicy::DenyAll => {
                config.push_str("net: NONE\n");
            }
            NetworkPolicy::AllowDomains(ref domains) => {
                config.push_str("net: RESTRICTED\n");
                for domain in domains {
                    config.push_str(&format!("allowed_domain: \"{}\"\n", domain));
                }
            }
            NetworkPolicy::AllowOutbound => {
                config.push_str("net: OUTGOING\n");
            }
            NetworkPolicy::AllowAll => {
                config.push_str("net: ALL\n");
            }
        }

        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_config_from_requirements() {
        let mut reqs = SandboxRequirements::default();
        reqs.max_memory_mb = 256;
        reqs.network_allowed = true;
        reqs.allowed_domains = vec!["api.example.com".to_string()];

        let config = ProcessSandboxConfig::from_requirements("test-1", &reqs, "/var/rootfs".into());

        assert_eq!(config.id, "test-1");
        assert_eq!(config.resource_limits.max_memory_bytes, 256 * 1024 * 1024);
        assert_eq!(config.uid, 65534);

        match config.network_policy {
            NetworkPolicy::AllowDomains(ref domains) => {
                assert_eq!(domains[0], "api.example.com");
            }
            _ => panic!("Expected AllowDomains"),
        }
    }

    #[test]
    fn test_nsjail_config_generation() {
        let reqs = SandboxRequirements::default();
        let config = ProcessSandboxConfig::from_requirements("test-2", &reqs, "/var/rootfs".into());

        let nsjail = config.to_nsjail_config();
        assert!(nsjail.contains("mode: ONCE"));
        assert!(nsjail.contains("rlimit_as"));
        assert!(nsjail.contains("net: NONE"));
    }
}
