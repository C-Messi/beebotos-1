#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{anyhow, Context};
use beebotos_launcher::{parse_launcher_command, LauncherCommand, WEB_CONSOLE_URL};
#[cfg(target_os = "windows")]
use beebotos_launcher::{read_env_file, write_env_file, EnvConfig};

const RUNNER_SCRIPT: &str = "beebotos-run.ps1";

fn main() {
    let command = parse_launcher_command(std::env::args().skip(1));
    if let Err(err) = run(command) {
        show_error("BeeBotOS Launcher", &err.to_string());
    }
}

fn run(command: LauncherCommand) -> anyhow::Result<()> {
    let root = app_root()?;
    match command {
        LauncherCommand::Ui => run_ui(root),
        LauncherCommand::Start => {
            run_runner(&root, "start")?;
            Ok(())
        }
        LauncherCommand::Stop => {
            run_runner(&root, "stop")?;
            Ok(())
        }
        LauncherCommand::Restart => {
            run_runner(&root, "restart")?;
            Ok(())
        }
        LauncherCommand::Status => {
            let output = run_runner(&root, "status")?;
            show_info("BeeBotOS 状态", &output);
            Ok(())
        }
        LauncherCommand::OpenWeb => open::that(WEB_CONSOLE_URL).context("打开 Web 控制台失败"),
        LauncherCommand::OpenLogs => open_logs(&root),
    }
}

fn app_root() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("读取 launcher 路径失败")?;
    Ok(exe
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(".")))
}

#[cfg(target_os = "windows")]
fn env_path(root: &Path) -> PathBuf {
    root.join(".env")
}

fn logs_path(root: &Path) -> PathBuf {
    root.join("data").join("logs")
}

fn runner_path(root: &Path) -> PathBuf {
    root.join(RUNNER_SCRIPT)
}

fn open_logs(root: &Path) -> anyhow::Result<()> {
    let path = logs_path(root);
    std::fs::create_dir_all(&path).context("创建日志目录失败")?;
    open::that(path).context("打开日志目录失败")
}

fn run_runner(root: &Path, action: &str) -> anyhow::Result<String> {
    let script = runner_path(root);
    if !script.exists() {
        return Err(anyhow!("找不到启动脚本: {}", script.display()));
    }

    let output = powershell_command()
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(&script)
        .arg(action)
        .arg("all")
        .current_dir(root)
        .output()
        .with_context(|| format!("执行 {} {} 失败", RUNNER_SCRIPT, action))?;

    command_output(output, action)
}

fn command_output(output: Output, action: &str) -> anyhow::Result<String> {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        if stdout.is_empty() {
            Ok(format!("BeeBotOS {} 已完成。", action))
        } else {
            Ok(stdout)
        }
    } else if stderr.is_empty() {
        Err(anyhow!("BeeBotOS {} 失败。", action))
    } else {
        Err(anyhow!("{}", stderr))
    }
}

fn powershell_command() -> Command {
    let mut command = Command::new("powershell.exe");
    hide_process_window(&mut command);
    command
}

#[cfg(target_os = "windows")]
fn hide_process_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
}

#[cfg(not(target_os = "windows"))]
fn hide_process_window(_command: &mut Command) {}

#[cfg(target_os = "windows")]
fn run_ui(root: PathBuf) -> anyhow::Result<()> {
    use std::rc::Rc;

    use native_windows_gui as nwg;

    nwg::init().context("初始化 Windows 窗口失败")?;
    nwg::Font::set_global_family("Segoe UI").context("设置窗口字体失败")?;

    let config = read_env_file(&env_path(&root)).unwrap_or_default();

    let mut window = Default::default();
    let mut title = Default::default();
    let mut hint = Default::default();
    let mut text_label = Default::default();
    let mut image_label = Default::default();
    let mut video_label = Default::default();
    let mut text_input = Default::default();
    let mut image_input = Default::default();
    let mut video_input = Default::default();
    let mut save_button = Default::default();
    let mut start_button = Default::default();
    let mut stop_button = Default::default();
    let mut restart_button = Default::default();
    let mut open_button = Default::default();
    let mut status_button = Default::default();
    let mut logs_button = Default::default();
    let mut footer = Default::default();

    nwg::Window::builder()
        .size((560, 390))
        .position((300, 220))
        .title("BeeBotOS Launcher")
        .build(&mut window)?;

    nwg::Label::builder()
        .text("BeeBotOS Launcher")
        .position((20, 16))
        .size((500, 28))
        .parent(&window)
        .build(&mut title)?;

    nwg::Label::builder()
        .text("配置密钥后点击保存；启动后会打开本机 Web 控制台。")
        .position((20, 45))
        .size((500, 22))
        .parent(&window)
        .build(&mut hint)?;

    build_label(&window, &mut text_label, "文本模型 Key", 82)?;
    build_label(&window, &mut image_label, "图像生成 Key", 122)?;
    build_label(&window, &mut video_label, "视频生成 Key", 162)?;
    build_input(&window, &mut text_input, &config.text_model_key, 78)?;
    build_input(&window, &mut image_input, &config.image_generation_key, 118)?;
    build_input(&window, &mut video_input, &config.video_generation_key, 158)?;

    build_button(&window, &mut save_button, "保存配置", 20, 210)?;
    build_button(&window, &mut start_button, "启动并打开", 150, 210)?;
    build_button(&window, &mut stop_button, "停止", 280, 210)?;
    build_button(&window, &mut restart_button, "重启", 410, 210)?;
    build_button(&window, &mut open_button, "打开 Web", 20, 260)?;
    build_button(&window, &mut status_button, "查看状态", 150, 260)?;
    build_button(&window, &mut logs_button, "打开日志", 280, 260)?;

    nwg::Label::builder()
        .text("配置写入安装目录 .env；脚本仍保留为内部运行器，普通用户无需操作 PowerShell。")
        .position((20, 326))
        .size((520, 28))
        .parent(&window)
        .build(&mut footer)?;

    let window = Rc::new(window);
    let events_window = window.clone();
    let events_root = root.clone();

    let handler = nwg::full_bind_event_handler(&window.handle, move |evt, _evt_data, handle| {
        use nwg::Event as E;

        if evt == E::OnWindowClose && &handle == &events_window as &nwg::Window {
            nwg::stop_thread_dispatch();
            return;
        }
        if evt != E::OnButtonClick {
            return;
        }

        if &handle == &save_button {
            let config = EnvConfig {
                text_model_key: text_input.text(),
                image_generation_key: image_input.text(),
                video_generation_key: video_input.text(),
            };
            show_action_result(
                &events_window,
                "保存配置",
                write_env_file(&env_path(&events_root), &config)
                    .map(|_| "配置已保存。".to_string()),
            );
        } else if &handle == &start_button {
            show_action_result(
                &events_window,
                "启动 BeeBotOS",
                run_runner(&events_root, "start").and_then(|output| {
                    open::that(WEB_CONSOLE_URL).context("打开 Web 控制台失败")?;
                    Ok(output)
                }),
            );
        } else if &handle == &stop_button {
            show_action_result(
                &events_window,
                "停止 BeeBotOS",
                run_runner(&events_root, "stop"),
            );
        } else if &handle == &restart_button {
            show_action_result(
                &events_window,
                "重启 BeeBotOS",
                run_runner(&events_root, "restart").and_then(|output| {
                    open::that(WEB_CONSOLE_URL).context("打开 Web 控制台失败")?;
                    Ok(output)
                }),
            );
        } else if &handle == &open_button {
            show_action_result(
                &events_window,
                "打开 Web",
                open::that(WEB_CONSOLE_URL)
                    .context("打开 Web 控制台失败")
                    .map(|_| "Web 控制台已打开。".to_string()),
            );
        } else if &handle == &status_button {
            show_action_result(
                &events_window,
                "BeeBotOS 状态",
                run_runner(&events_root, "status"),
            );
        } else if &handle == &logs_button {
            show_action_result(
                &events_window,
                "打开日志",
                open_logs(&events_root).map(|_| "日志目录已打开。".to_string()),
            );
        }
    });

    nwg::dispatch_thread_events();
    nwg::unbind_event_handler(&handler);
    Ok(())
}

#[cfg(target_os = "windows")]
fn build_label(
    window: &native_windows_gui::Window,
    label: &mut native_windows_gui::Label,
    text: &str,
    y: i32,
) -> Result<(), native_windows_gui::NwgError> {
    native_windows_gui::Label::builder()
        .text(text)
        .position((20, y))
        .size((110, 24))
        .parent(window)
        .build(label)
}

#[cfg(target_os = "windows")]
fn build_input(
    window: &native_windows_gui::Window,
    input: &mut native_windows_gui::TextInput,
    text: &str,
    y: i32,
) -> Result<(), native_windows_gui::NwgError> {
    native_windows_gui::TextInput::builder()
        .text(text)
        .password(Some('*'))
        .position((140, y))
        .size((380, 28))
        .parent(window)
        .build(input)
}

#[cfg(target_os = "windows")]
fn build_button(
    window: &native_windows_gui::Window,
    button: &mut native_windows_gui::Button,
    text: &str,
    x: i32,
    y: i32,
) -> Result<(), native_windows_gui::NwgError> {
    native_windows_gui::Button::builder()
        .text(text)
        .position((x, y))
        .size((110, 34))
        .parent(window)
        .build(button)
}

#[cfg(target_os = "windows")]
fn show_action_result(
    window: &native_windows_gui::Window,
    title: &str,
    result: anyhow::Result<String>,
) {
    match result {
        Ok(message) => native_windows_gui::modal_info_message(window, title, &message),
        Err(err) => native_windows_gui::modal_error_message(window, title, &err.to_string()),
    };
}

#[cfg(not(target_os = "windows"))]
fn run_ui(_root: PathBuf) -> anyhow::Result<()> {
    println!("BeeBotOS Launcher GUI is available on Windows. Use beebotos-run.ps1 on Windows.");
    Ok(())
}

#[cfg(target_os = "windows")]
fn show_info(title: &str, message: &str) {
    let _ = native_windows_gui::init();
    native_windows_gui::simple_message(title, message);
}

#[cfg(not(target_os = "windows"))]
fn show_info(title: &str, message: &str) {
    println!("{title}\n{message}");
}

#[cfg(target_os = "windows")]
fn show_error(title: &str, message: &str) {
    let _ = native_windows_gui::init();
    native_windows_gui::simple_message(title, message);
}

#[cfg(not(target_os = "windows"))]
fn show_error(title: &str, message: &str) {
    eprintln!("{title}: {message}");
}
