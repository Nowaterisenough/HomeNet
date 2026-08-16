use crate::config::add_log;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::path::PathBuf;

#[cfg(target_os = "macos")]
const MACOS_LAUNCH_AGENT_LABEL: &str = "com.nowat.homenet";
#[cfg(target_os = "macos")]
const LEGACY_MACOS_LAUNCH_AGENT_LABELS: &[&str] = &["com.nowat.home-net"];
#[cfg(target_os = "macos")]
const REMOVE_LEGACY_MACOS_LOGIN_ITEMS_SCRIPT: &str = r#"
tell application "System Events"
    if exists login item "HomeNet" then
        delete login item "HomeNet"
    end if
    if exists login item "homenet" then
        delete login item "homenet"
    end if
    if exists login item "网络管家" then
        delete login item "网络管家"
    end if
end tell
"#;

/// Enable or disable automatic startup at OS login.
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        set_autostart_windows(enabled)
    }
    #[cfg(target_os = "linux")]
    {
        set_autostart_linux(enabled)
    }
    #[cfg(target_os = "macos")]
    {
        set_autostart_macos(enabled)
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        Err("当前平台不支持自启动".into())
    }
}

/// Check whether auto-start is currently enabled.
pub fn is_autostart_enabled() -> bool {
    #[cfg(target_os = "windows")]
    {
        is_autostart_enabled_windows()
    }
    #[cfg(target_os = "linux")]
    {
        is_autostart_enabled_linux()
    }
    #[cfg(target_os = "macos")]
    {
        is_autostart_enabled_macos()
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        false
    }
}

// ---------------------------------------------------------------------------
// Windows – registry Run key
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[cfg(target_os = "windows")]
fn command_output_without_window(
    command: &mut std::process::Command,
) -> std::io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;

    command.creation_flags(CREATE_NO_WINDOW).output()
}

#[cfg(target_os = "windows")]
fn set_autostart_windows(enabled: bool) -> Result<(), String> {
    let exe_path = std::env::current_exe().map_err(|e| format!("无法获取程序路径: {}", e))?;
    let exe_str = exe_path.to_string_lossy().to_string();

    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let value_name = "HomeNet";

    if enabled {
        let mut command = std::process::Command::new("reg");
        command.args([
            "add", key, "/v", value_name, "/t", "REG_SZ", "/d", &exe_str, "/f",
        ]);
        let output = command_output_without_window(&mut command)
            .map_err(|e| format!("注册表写入失败: {}", e))?;

        if output.status.success() {
            add_log("info", "自启动", "已写入 Windows 启动项");
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("注册表写入失败: {}", stderr))
        }
    } else {
        let mut command = std::process::Command::new("reg");
        command.args(["delete", key, "/v", value_name, "/f"]);
        let output = command_output_without_window(&mut command)
            .map_err(|e| format!("注册表删除失败: {}", e))?;

        if output.status.success() || !is_autostart_enabled_windows() {
            add_log("info", "自启动", "已移除 Windows 启动项");
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("注册表删除失败: {}", stderr))
        }
    }
}

#[cfg(target_os = "windows")]
fn is_autostart_enabled_windows() -> bool {
    let mut command = std::process::Command::new("reg");
    command.args([
        "query",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
        "/v",
        "HomeNet",
    ]);
    let output = command_output_without_window(&mut command)
        .map(|o| o.status.success())
        .unwrap_or(false);
    output
}

// ---------------------------------------------------------------------------
// Linux – XDG autostart .desktop file
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn autostart_desktop_path() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".config")
        });
    base.join("autostart").join("homenet.desktop")
}

#[cfg(target_os = "linux")]
fn set_autostart_linux(enabled: bool) -> Result<(), String> {
    let path = autostart_desktop_path();

    if enabled {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("无法创建 autostart 目录: {}", e))?;
        }

        let exe = std::env::current_exe()
            .map_err(|e| format!("无法获取程序路径: {}", e))?;

        let content = format!(
            r#"[Desktop Entry]
Type=Application
Name=HomeNet
Comment=DDNS 与端口转发
Exec={}
Terminal=false
X-GNOME-Autostart-enabled=true
"#,
            exe.to_string_lossy()
        );

        std::fs::write(&path, content)
            .map_err(|e| format!("写入 autostart 文件失败: {}", e))?;

        add_log("info", "自启动", "已写入 Linux 自启动文件");
        Ok(())
    } else {
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("删除 autostart 文件失败: {}", e))?;
        }
        add_log("info", "自启动", "已移除 Linux 自启动文件");
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn is_autostart_enabled_linux() -> bool {
    autostart_desktop_path().exists()
}

// ---------------------------------------------------------------------------
// macOS – LaunchAgent plist
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn launchagent_plist_path_for_home(home: impl Into<PathBuf>, label: &str) -> PathBuf {
    home.into()
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{}.plist", label))
}

#[cfg(target_os = "macos")]
fn launchagent_plist_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    launchagent_plist_path_for_home(home, MACOS_LAUNCH_AGENT_LABEL)
}

#[cfg(target_os = "macos")]
fn remove_launchagent_file(path: &std::path::Path) -> Result<(), String> {
    if path.exists() {
        std::fs::remove_file(path)
            .map_err(|e| format!("删除 LaunchAgent 文件失败（{}）：{}", path.display(), e))?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn remove_legacy_macos_launchagents() -> Result<(), String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    for label in LEGACY_MACOS_LAUNCH_AGENT_LABELS {
        let path = launchagent_plist_path_for_home(&home, label);
        remove_launchagent_file(&path)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn remove_legacy_macos_login_items() -> Result<(), String> {
    let output = std::process::Command::new("osascript")
        .args(["-e", REMOVE_LEGACY_MACOS_LOGIN_ITEMS_SCRIPT])
        .output()
        .map_err(|e| format!("检查 macOS 登录项失败：{}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!("清理 macOS 遗留登录项失败：{}", stderr))
    }
}

#[cfg(target_os = "macos")]
fn write_macos_launchagent() -> Result<(), String> {
    let path = launchagent_plist_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("无法创建 LaunchAgents 目录: {}", e))?;
    }

    let exe = std::env::current_exe().map_err(|e| format!("无法获取程序路径: {}", e))?;

    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>Program</key>
    <string>{}</string>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>"#,
        MACOS_LAUNCH_AGENT_LABEL,
        exe.to_string_lossy()
    );

    std::fs::write(&path, content).map_err(|e| format!("写入 LaunchAgent 文件失败: {}", e))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn set_autostart_macos(enabled: bool) -> Result<(), String> {
    // HomeNet historically also appeared in macOS Login Items. Remove those
    // entries first so the app has exactly one source of automatic launches.
    remove_legacy_macos_login_items()?;
    remove_legacy_macos_launchagents()?;

    if enabled {
        write_macos_launchagent()?;
        add_log("info", "自启动", "已写入 macOS LaunchAgent");
        Ok(())
    } else {
        remove_launchagent_file(&launchagent_plist_path())?;
        add_log("info", "自启动", "已移除 macOS LaunchAgent");
        Ok(())
    }
}

/// Reconcile historical macOS startup registrations during an app upgrade.
///
/// LaunchAgent is the sole supported mechanism. Legacy Login Items and the
/// pre-rename LaunchAgent are removed before the persisted setting is applied.
#[cfg(target_os = "macos")]
pub fn migrate_macos_autostart(enabled: bool) -> Result<(), String> {
    remove_legacy_macos_login_items()?;
    remove_legacy_macos_launchagents()?;

    if enabled {
        // Keep an existing installed-app path intact. In particular, a
        // developer running a debug build must not replace it with target/debug.
        if !launchagent_plist_path().exists() {
            write_macos_launchagent()?;
        }
    } else {
        remove_launchagent_file(&launchagent_plist_path())?;
    }

    add_log("info", "自启动", "macOS 自启动项迁移检查完成");
    Ok(())
}

#[cfg(target_os = "macos")]
fn is_autostart_enabled_macos() -> bool {
    launchagent_plist_path().exists()
}

#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    use super::{
        launchagent_plist_path_for_home, LEGACY_MACOS_LAUNCH_AGENT_LABELS,
        MACOS_LAUNCH_AGENT_LABEL, REMOVE_LEGACY_MACOS_LOGIN_ITEMS_SCRIPT,
    };
    use std::path::Path;

    #[test]
    fn canonical_launchagent_path_uses_current_identifier() {
        assert_eq!(
            launchagent_plist_path_for_home("/Users/test", MACOS_LAUNCH_AGENT_LABEL),
            Path::new("/Users/test/Library/LaunchAgents/com.nowat.homenet.plist")
        );
    }

    #[test]
    fn migration_tracks_pre_rename_launchagent() {
        assert_eq!(LEGACY_MACOS_LAUNCH_AGENT_LABELS, &["com.nowat.home-net"]);
    }

    #[test]
    fn migration_removes_all_historical_login_item_names() {
        for name in ["HomeNet", "homenet", "网络管家"] {
            assert!(REMOVE_LEGACY_MACOS_LOGIN_ITEMS_SCRIPT.contains(name));
        }
    }
}
