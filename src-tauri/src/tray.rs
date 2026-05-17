use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};

/// Create and attach a system-tray icon with a context menu.
///
/// The menu contains:
/// * **显示窗口** – show and focus the main application window.
/// * **退出**     – fully exit the application.
pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // ---- menu items --------------------------------------------------
    let show_item = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    // ---- tray icon ---------------------------------------------------
    let _tray = TrayIconBuilder::new()
        .icon(
            app.default_window_icon()
                .cloned()
                .unwrap_or_else(|| app.default_window_icon().unwrap().clone()),
        )
        .menu(&menu)
        .tooltip("网络管家 · DDNS与端口转发")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                crate::config::add_log("info", "托盘", "用户从系统托盘退出应用");
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}
