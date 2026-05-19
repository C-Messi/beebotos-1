//! Cross-platform subprocess environment handling for shell/CLI tools.
//!
//! BeeBotOS keeps file operations workspace-aware, but external CLIs such as
//! `gh`, `aws`, `kubectl`, and exchange CLIs usually discover their config via
//! the host user's HOME/USERPROFILE/APPDATA/XDG variables.  This module keeps
//! that user CLI environment available while still exposing BEEBOTOS_WORKSPACE.

use std::path::Path;

/// Configure a subprocess to run like the host user's CLI session.
///
/// This intentionally does not set HOME to the BeeBotOS workspace. The
/// workspace should be represented by current_dir/BEEBOTOS_WORKSPACE, while
/// OS-specific user-home variables remain available for CLIs that need their
/// existing profiles and credentials.
pub(crate) fn configure_host_user_cli_environment(
    cmd: &mut tokio::process::Command,
    workspace: Option<&Path>,
) {
    cmd.env_clear();
    apply_host_user_env(cmd);
    cmd.env("PATH", process_path());

    if let Some(workspace) = workspace {
        cmd.env("BEEBOTOS_WORKSPACE", workspace);
    }
}

fn apply_host_user_env(cmd: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        for key in [
            "USERPROFILE",
            "HOMEDRIVE",
            "HOMEPATH",
            "APPDATA",
            "LOCALAPPDATA",
            "USERNAME",
            "USERDOMAIN",
            "TEMP",
            "TMP",
            "PATHEXT",
            "SystemRoot",
            "WINDIR",
            "COMSPEC",
            "ProgramData",
            "PROGRAMDATA",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ProgramW6432",
            "PSModulePath",
        ] {
            copy_env_if_present(cmd, key);
        }
    }

    #[cfg(not(windows))]
    {
        for key in [
            "HOME",
            "USER",
            "LOGNAME",
            "SHELL",
            "LANG",
            "LANGUAGE",
            "TERM",
            "COLORTERM",
            "TZ",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            "SSH_AUTH_SOCK",
        ] {
            copy_env_if_present(cmd, key);
        }
        copy_env_prefix(cmd, "LC_");
    }
}

fn copy_env_if_present(cmd: &mut tokio::process::Command, key: &str) {
    if let Ok(value) = std::env::var(key) {
        cmd.env(key, value);
    }
}

#[cfg(not(windows))]
fn copy_env_prefix(cmd: &mut tokio::process::Command, prefix: &str) {
    for (key, value) in std::env::vars() {
        if key.starts_with(prefix) {
            cmd.env(key, value);
        }
    }
}

fn process_path() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    #[cfg(windows)]
    {
        merge_windows_path(current)
    }
    #[cfg(not(windows))]
    {
        current
    }
}

#[cfg(windows)]
fn merge_windows_path(current: String) -> String {
    let mut entries = Vec::new();
    for key in [
        r"HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
        r"HKEY_CURRENT_USER\Environment",
    ] {
        if let Ok(output) = std::process::Command::new("reg")
            .args(["query", key, "/v", "Path"])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let trimmed = line.trim();
                    if !trimmed.starts_with("Path") {
                        continue;
                    }
                    if let Some(value) = parse_reg_path_value(trimmed) {
                        entries.extend(
                            std::env::split_paths(&value).map(|p| p.to_string_lossy().to_string()),
                        );
                    }
                }
            }
        }
    }
    entries.extend(std::env::split_paths(&current).map(|p| p.to_string_lossy().to_string()));

    let mut seen = std::collections::HashSet::new();
    let mut merged = Vec::new();
    for entry in entries {
        if entry.trim().is_empty() {
            continue;
        }
        let key = entry.trim_end_matches(['\\', '/']).to_ascii_lowercase();
        if seen.insert(key) {
            merged.push(entry);
        }
    }
    std::env::join_paths(merged)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(current)
}

#[cfg(windows)]
fn parse_reg_path_value(line: &str) -> Option<String> {
    let marker = if line.contains("REG_EXPAND_SZ") {
        "REG_EXPAND_SZ"
    } else if line.contains("REG_SZ") {
        "REG_SZ"
    } else {
        return None;
    };
    let value = line.split_once(marker)?.1.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[cfg(not(windows))]
    #[tokio::test]
    async fn host_user_environment_preserves_home_and_marks_workspace() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let old_home = std::env::var_os("HOME");
        let old_path = std::env::var_os("PATH");
        let temp_home = tempfile::tempdir().expect("home tempdir");
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        std::env::set_var("HOME", temp_home.path());
        std::env::set_var("PATH", "/usr/bin:/bin");

        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c")
            .arg("printf '%s|%s' \"$HOME\" \"$BEEBOTOS_WORKSPACE\"");
        configure_host_user_cli_environment(&mut cmd, Some(workspace.path()));

        let output = cmd.output().await.expect("run shell");
        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
        assert_eq!(
            stdout,
            format!(
                "{}|{}",
                temp_home.path().display(),
                workspace.path().display()
            )
        );

        restore_env("HOME", old_home);
        restore_env("PATH", old_path);
    }

    #[cfg(not(windows))]
    fn restore_env(key: &str, value: Option<std::ffi::OsString>) {
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}
