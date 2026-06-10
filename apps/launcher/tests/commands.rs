use beebotos_launcher::{parse_launcher_command, LauncherCommand};

#[test]
fn launcher_opens_ui_without_args() {
    assert_eq!(
        parse_launcher_command(Vec::<String>::new()),
        LauncherCommand::Ui
    );
}

#[test]
fn launcher_accepts_installer_friendly_commands() {
    assert_eq!(parse_launcher_command(["--start"]), LauncherCommand::Start);
    assert_eq!(parse_launcher_command(["stop"]), LauncherCommand::Stop);
    assert_eq!(
        parse_launcher_command(["--restart"]),
        LauncherCommand::Restart
    );
    assert_eq!(parse_launcher_command(["status"]), LauncherCommand::Status);
    assert_eq!(parse_launcher_command(["--open"]), LauncherCommand::OpenWeb);
    assert_eq!(parse_launcher_command(["logs"]), LauncherCommand::OpenLogs);
}
