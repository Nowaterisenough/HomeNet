use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayIconAction {
    RestoreMainWindow,
    Ignore,
}

const TRAY_TOOLTIP: &str = "HomeNet";

fn tray_icon_action(event: &TrayIconEvent) -> TrayIconAction {
    match event {
        TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        } => TrayIconAction::RestoreMainWindow,
        _ => TrayIconAction::Ignore,
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

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
        .tooltip(TRAY_TOOLTIP)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                show_main_window(app);
            }
            "quit" => {
                crate::config::add_log("info", "托盘", "用户从系统托盘退出应用");
                app.exit(0);
            }
            _ => {}
        })
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if tray_icon_action(&event) == TrayIconAction::RestoreMainWindow {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{tray_icon_action, TrayIconAction, TRAY_TOOLTIP};
    use tauri::{
        tray::{MouseButton, MouseButtonState, TrayIconEvent, TrayIconId},
        PhysicalPosition, PhysicalSize, Position, Rect, Size,
    };

    fn empty_rect() -> Rect {
        Rect {
            size: Size::Physical(PhysicalSize::new(0, 0)),
            position: Position::Physical(PhysicalPosition::new(0, 0)),
        }
    }

    fn left_double_click_event() -> TrayIconEvent {
        TrayIconEvent::DoubleClick {
            id: TrayIconId::new("main"),
            position: PhysicalPosition::new(0.0, 0.0),
            rect: empty_rect(),
            button: MouseButton::Left,
        }
    }

    #[test]
    fn windows_tray_left_double_click_restores_main_window() {
        assert_eq!(
            tray_icon_action(&left_double_click_event()),
            TrayIconAction::RestoreMainWindow
        );
    }

    #[test]
    fn macos_tray_left_double_click_restores_main_window() {
        assert_eq!(
            tray_icon_action(&left_double_click_event()),
            TrayIconAction::RestoreMainWindow
        );
    }

    #[test]
    fn tray_tooltip_uses_product_name_only() {
        assert_eq!(TRAY_TOOLTIP, "HomeNet");
    }

    #[test]
    fn right_double_click_does_not_restore_main_window() {
        let event = TrayIconEvent::DoubleClick {
            id: TrayIconId::new("main"),
            position: PhysicalPosition::new(0.0, 0.0),
            rect: empty_rect(),
            button: MouseButton::Right,
        };

        assert_eq!(tray_icon_action(&event), TrayIconAction::Ignore);
    }

    #[test]
    fn regular_click_does_not_restore_main_window() {
        let event = TrayIconEvent::Click {
            id: TrayIconId::new("main"),
            position: PhysicalPosition::new(0.0, 0.0),
            rect: empty_rect(),
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
        };

        assert_eq!(tray_icon_action(&event), TrayIconAction::Ignore);
    }
}
