//! Self-update command for BeeBotOS CLI

use std::env;
use std::path::Path;

use beebotos_update_client::client::{ConsoleProgress, NativeUpdateClient, UpdateClient};
use beebotos_update_client::config::UpdateConfig;
use beebotos_update_client::models::{UpdateStatus, VersionInfo};
use clap::Args;

/// Update CLI arguments
#[derive(Args)]
pub struct UpdateArgs {
    /// Force update, skip confirmation
    #[arg(long)]
    force: bool,

    /// Check update but don't install
    #[arg(long)]
    check: bool,

    /// Rollback to previous version
    #[arg(long)]
    rollback: bool,

    /// Specify target version
    #[arg(long)]
    version: Option<String>,

    /// Update server URL
    #[arg(long, env = "BEEBOTOS_UPDATE_SERVER")]
    server: Option<String>,

    /// Update channel
    #[arg(long, default_value = "stable")]
    channel: String,
}

/// Execute update command
pub async fn execute(args: UpdateArgs) -> anyhow::Result<()> {
    if args.rollback {
        return perform_rollback().await;
    }

    let mut config = UpdateConfig::from_env().with_app_name("cli");

    if let Some(server) = &args.server {
        config.server_url = server.clone();
    }
    config.channel = args.channel;

    let client = NativeUpdateClient::new(config)?;

    println!("Checking for updates...");
    let info = match client.check_update().await? {
        Some(info) => info,
        None => {
            println!("Already up to date!");
            return Ok(());
        }
    };

    print_update_info(&info);

    if args.check {
        return Ok(());
    }

    // Confirm update
    if !args.force && !info.mandatory {
        if !confirm_update(&info)? {
            println!("Update cancelled.");
            return Ok(());
        }
    }

    // Execute self-update
    perform_self_update(client, info).await
}

fn print_update_info(info: &VersionInfo) {
    println!("New version available: {}", info.version);
    if let Some(note) = info.release_notes.get("zh") {
        println!("Release notes (zh): {}", note);
    } else if let Some(note) = info.release_notes.get("en") {
        println!("Release notes (en): {}", note);
    }
    if info.mandatory {
        println!("⚠️  This is a MANDATORY update!");
    }
    println!("Priority: {:?}", info.priority);
}

fn confirm_update(info: &VersionInfo) -> anyhow::Result<bool> {
    use dialoguer::Confirm;
    let prompt = if info.mandatory {
        "This update is mandatory. Install now?"
    } else {
        "Do you want to install this update?"
    };
    Ok(Confirm::new()
        .with_prompt(prompt)
        .default(true)
        .interact()?)
}

async fn perform_self_update(client: NativeUpdateClient, info: VersionInfo) -> anyhow::Result<()> {
    let package = select_package(&info.packages)?;

    // Download
    println!("Downloading update...");
    let progress = ConsoleProgress;
    let temp_path = client.download(&package, &progress).await?;

    // Verify
    println!("Verifying package...");
    if !client.verify(&temp_path, &package).await? {
        return Err(anyhow::anyhow!("Package verification failed"));
    }

    // Get current executable path
    let current_exe = env::current_exe()?;
    let backup_path = get_backup_path(&current_exe);

    // Backup
    println!("Creating backup...");
    tokio::fs::copy(&current_exe, &backup_path).await?;

    // Replace binary
    println!("Installing update...");
    replace_binary(&temp_path, &current_exe).await?;

    // Clean up temp file
    let _ = tokio::fs::remove_file(&temp_path).await;

    // Report success
    let _ = client
        .report_status(&info.version.to_string(), UpdateStatus::Completed, 0, None)
        .await;

    println!("Update completed successfully!");
    println!("New version: {}", info.version);
    println!("Please restart beebot to use the new version.");

    Ok(())
}

fn select_package(
    packages: &[beebotos_update_client::models::PackageInfo],
) -> anyhow::Result<beebotos_update_client::models::PackageInfo> {
    beebotos_update_client::select_package(packages)
        .map_err(|e| anyhow::anyhow!("No suitable package found for platform: {}", e))
}

fn get_backup_path(current_exe: &Path) -> std::path::PathBuf {
    let mut backup = current_exe.as_os_str().to_os_string();
    backup.push(".backup");
    backup.into()
}

#[cfg(unix)]
async fn replace_binary(source: &Path, target: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    // Set executable permissions
    let mut perms = tokio::fs::metadata(source).await?.permissions();
    perms.set_mode(0o755);
    tokio::fs::set_permissions(source, perms).await?;

    // Atomic replace
    tokio::fs::rename(source, target).await?;

    Ok(())
}

#[cfg(windows)]
async fn replace_binary(source: &Path, target: &Path) -> anyhow::Result<()> {
    // Windows: file may be locked, need special handling
    // For simplicity, rename old binary and move new one in place
    let old_path = target.with_extension("old");
    tokio::fs::rename(target, &old_path).await?;
    tokio::fs::rename(source, target).await?;
    println!("Old binary saved to: {}", old_path.display());
    println!("Please remove the .old file after confirming the update works.");
    Ok(())
}

async fn perform_rollback() -> anyhow::Result<()> {
    let current_exe = env::current_exe()?;
    let backup_path = get_backup_path(&current_exe);

    if !backup_path.exists() {
        return Err(anyhow::anyhow!("No backup found to rollback to"));
    }

    println!("Rolling back to previous version...");
    tokio::fs::copy(&backup_path, &current_exe).await?;
    println!("Rollback completed. Please restart beebot.");

    Ok(())
}
