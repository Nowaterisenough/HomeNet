use crate::config::add_log;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::path::PathBuf;

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
fn set_autostart_windows(enabled: bool) -> Result<(), String> {
    let exe_path = std::env::current_exe()
        .map_err(|e| format!("无法获取程序路径: {}", e))?;
    let exe_str = exe_path.to_string_lossy().to_string();

    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let value_name = "HomeNet";

    if enabled {
        let output = std::process::Command::new("reg")
            .args(["add", key, "/v", value_name, "/t", "REG_SZ", "/d", &exe_str, "/f"])
            .output()
            .map_err(|e| format!("注册表写入失败: {}", e))?;

        if output.status.success() {
            add_log("info", "自启动", "已写入 Windows 启动项");
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("注册表写入失败: {}", stderr))
        }
    } else {
        let output = std::process::Command::new("reg")
            .args(["delete", key, "/v", value_name, "/f"])
            .output()
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
    let output = std::process::Command::new("reg")
        .args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "HomeNet"])
        .output()
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
    base.join("autostart").join("home-net.desktop")
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
Name=网络管家
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
fn launchagent_plist_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join("com.nowat.home-net.plist")
}

#[cfg(target_os = "macos")]
fn set_autostart_macos(enabled: bool) -> Result<(), String> {
    let path = launchagent_plist_path();

    if enabled {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("无法创建 LaunchAgents 目录: {}", e))?;
        }

        let exe = std::env::current_exe()
            .map_err(|e| format!("无法获取程序路径: {}", e))?;

        let content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.nowat.home-net</string>
    <key>Program</key>
    <string>{}</string>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>"#,
            exe.to_string_lossy()
        );

        std::fs::write(&path, content)
            .map_err(|e| format!("写入 LaunchAgent 文件失败: {}", e))?;

        add_log("info", "自启动", "已写入 macOS LaunchAgent");
        Ok(())
    } else {
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("删除 LaunchAgent 文件失败: {}", e))?;
        }
        add_log("info", "自启动", "已移除 macOS LaunchAgent");
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn is_autostart_enabled_macos() -> bool {
    launchagent_plist_path().exists()
}
