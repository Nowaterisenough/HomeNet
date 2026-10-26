use tauri::{WebviewWindow, WindowEvent};

pub(crate) fn install_close_handler(window: &WebviewWindow) {
    let current = window.clone();
    #[cfg(target_os = "macos")]
    let hide_after_fullscreen = std::sync::atomic::AtomicBool::new(false);

    window.on_window_event(move |event| {
        #[cfg(target_os = "macos")]
        use std::sync::atomic::Ordering;

        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            #[cfg(target_os = "macos")]
            if current.is_fullscreen().unwrap_or(false) {
                // Leave the native full-screen Space before hiding the window.
                hide_after_fullscreen.store(true, Ordering::Relaxed);
                if let Err(error) = current.set_fullscreen(false) {
                    hide_after_fullscreen.store(false, Ordering::Relaxed);
                    crate::config::add_log(
                        "warn",
                        "系统",
                        &format!("退出全屏失败，保留窗口：{error}"),
                    );
                }
                return;
            }
            let _ = current.hide();
            crate::config::add_log("debug", "系统", "主窗口已隐藏到系统托盘");
        }
        #[cfg(target_os = "macos")]
        if matches!(event, WindowEvent::Resized(_))
            && hide_after_fullscreen.load(Ordering::Relaxed)
            && !current.is_fullscreen().unwrap_or(true)
        {
            hide_after_fullscreen.store(false, Ordering::Relaxed);
            let _ = current.hide();
            crate::config::add_log("debug", "系统", "主窗口退出全屏后已隐藏到系统托盘");
        }
    });
}
