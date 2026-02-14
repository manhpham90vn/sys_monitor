use std::thread;
use std::time::Duration;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::App;

use crate::metrics::SystemMetrics;

/// Sets up the system tray icon, context menu, and background metrics thread.
pub fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    // ── Tray context menu ──────────────────────────────────────
    let login_i = CheckMenuItem::with_id(
        app,
        "start_on_login",
        "Start on Login",
        true,
        false,
        None::<&str>,
    )?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&login_i, &sep, &quit_i])?;

    // ── Tray icon ──────────────────────────────────────────────
    let tray = TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("sys_monitor")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "start_on_login" => {
                // TODO: Implement XDG autostart desktop file creation/removal
                println!("Start on Login toggled (placeholder)");
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    // ── Background metrics collection thread ───────────────────
    let tray_c = tray.clone();

    thread::spawn(move || {
        let mut metrics = SystemMetrics::new();

        // Initial baseline delay for CPU usage calculation
        thread::sleep(Duration::from_millis(500));

        loop {
            metrics.refresh();
            let label = metrics.format_label();

            let _ = tray_c.set_tooltip(Some(label));
            let _ = tray_c.set_title(Some(label));

            thread::sleep(Duration::from_secs(1));
        }
    });

    Ok(())
}
