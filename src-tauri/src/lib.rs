use std::thread;
use std::time::Duration;
use sysinfo::{Components, Disks, Networks, System};

use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

/// Formats a byte count into a compact human-readable rate string.
///
/// - >= 1 MB  → "1.2M"
/// - >= 1 KB  → "300K"
/// - < 1 KB   → "42B"
fn fmt_rate(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1}M", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.0}K", bytes as f64 / 1_000.0)
    } else {
        format!("{}B", bytes)
    }
}

/// Attempts to read the CPU package/die temperature from hardware sensors.
///
/// Searches through available `Components` for labels commonly used by
/// Intel (`coretemp` → "Package id 0") and AMD (`k10temp` → "Tctl")
/// desktop sensors. Falls back to the first component reporting a
/// temperature above 0 °C.
///
/// Returns `None` if no temperature sensor is available (e.g. missing
/// `lm-sensors` or running in a VM).
fn cpu_temp(components: &Components) -> Option<f32> {
    // Priority order: Package temp (Intel), Tctl (AMD), generic CPU, individual cores
    let candidates = ["Package", "Tctl", "CPU", "Core 0", "Core"];
    for keyword in &candidates {
        for c in components.iter() {
            if c.label().contains(keyword) {
                if let Some(t) = c.temperature() {
                    return Some(t);
                }
            }
        }
    }
    // Last resort: any sensor with temperature > 0
    components
        .iter()
        .find_map(|c| c.temperature().filter(|&t| t > 0.0))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // ── Tray context menu ──────────────────────────────────────
            // "Start on Login" is a UI-only checkbox for now (placeholder).
            // No actual autostart logic is implemented yet.
            let login_i = CheckMenuItem::with_id(
                app,
                "start_on_login",
                "Start on Login",
                true,  // enabled
                false, // initially unchecked
                None::<&str>,
            )?;
            let sep = PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&login_i, &sep, &quit_i])?;

            // ── Tray icon ──────────────────────────────────────────────
            // Uses the default window icon (configured in tauri.conf.json → bundle.icon).
            // On GNOME, the icon is rendered via libayatana-appindicator.
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
            // Spawns a dedicated thread that polls system metrics every 1 second
            // and updates the tray icon's title (panel text) and tooltip.
            let tray_c = tray.clone();

            thread::spawn(move || {
                // Initialize all subsystems we need: CPU, memory, components, etc.
                let mut sys = System::new_all();
                let mut nets = Networks::new_with_refreshed_list();
                let disks = Disks::new_with_refreshed_list();
                let mut components = Components::new_with_refreshed_list();

                // sysinfo requires two consecutive CPU usage refreshes to calculate
                // a meaningful delta. The first call establishes the baseline.
                sys.refresh_cpu_usage();
                thread::sleep(Duration::from_millis(500));

                // Track cumulative network bytes to compute per-second speed.
                // On each tick we calculate: current_total - previous_total = bytes/sec.
                let mut prev_rx: u64 = nets.iter().map(|(_, n)| n.total_received()).sum();
                let mut prev_tx: u64 = nets.iter().map(|(_, n)| n.total_transmitted()).sum();

                loop {
                    // Refresh only the subsystems we actually read from.
                    sys.refresh_cpu_usage();
                    sys.refresh_memory();
                    nets.refresh(false); // false = don't remove disconnected interfaces
                    components.refresh(false); // false = don't remove disappeared sensors

                    // ── CPU usage (%) ──────────────────────────────────
                    // Returns the average utilization across all logical cores (0–100).
                    let cpu = sys.global_cpu_usage();

                    // ── CPU temperature (°C) ───────────────────────────
                    // May return None on systems without hwmon/lm-sensors support.
                    let temp = cpu_temp(&components);

                    // ── RAM usage (%) ──────────────────────────────────
                    // used_memory() / total_memory(), both in bytes.
                    let total_mem = sys.total_memory();
                    let ram = if total_mem > 0 {
                        (sys.used_memory() as f64 / total_mem as f64 * 100.0) as f32
                    } else {
                        0.0
                    };

                    // ── Swap usage (%) ─────────────────────────────────
                    let total_swap = sys.total_swap();
                    let swap = if total_swap > 0 {
                        (sys.used_swap() as f64 / total_swap as f64 * 100.0) as f32
                    } else {
                        0.0
                    };

                    // ── Load average (1-minute) ────────────────────────
                    // Number of processes waiting for CPU or I/O. A value greater
                    // than the number of CPU cores indicates the system is overloaded.
                    let load = System::load_average().one;

                    // ── Disk usage (%) ─────────────────────────────────
                    // Sums total_space and available_space across ALL mounted filesystems.
                    let disk_total: u64 = disks.iter().map(|d| d.total_space()).sum();
                    let disk_avail: u64 = disks.iter().map(|d| d.available_space()).sum();
                    let disk_pct = if disk_total > 0 {
                        ((disk_total - disk_avail) as f64 / disk_total as f64 * 100.0) as f32
                    } else {
                        0.0
                    };

                    // ── Network throughput (bytes/sec) ─────────────────
                    // Calculate delta since the last tick (≈ 1 second) to derive speed.
                    let cur_rx: u64 = nets.iter().map(|(_, n)| n.total_received()).sum();
                    let cur_tx: u64 = nets.iter().map(|(_, n)| n.total_transmitted()).sum();
                    let dl = cur_rx.saturating_sub(prev_rx);
                    let ul = cur_tx.saturating_sub(prev_tx);
                    prev_rx = cur_rx;
                    prev_tx = cur_tx;

                    // ── Compose the panel label ────────────────────────
                    // Temperature is optional — only appended if a sensor was found.
                    let temp_str = match temp {
                        Some(t) => format!(" {:.0}°C", t),
                        None => String::new(),
                    };

                    let label = format!(
                        "CPU {:.0}%{} | RAM {:.0}% | Swap {:.0}% | Load Average {:.1} | Disk {:.0}% | Net ↓{} ↑{}",
                        cpu, temp_str, ram, swap, load, disk_pct, fmt_rate(dl), fmt_rate(ul)
                    );

                    // set_title() maps to app_indicator_set_label() on Linux,
                    // which renders the text on the GNOME top panel next to the icon.
                    let _ = tray_c.set_tooltip(Some(&label));
                    let _ = tray_c.set_title(Some(&label));

                    thread::sleep(Duration::from_secs(1));
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
