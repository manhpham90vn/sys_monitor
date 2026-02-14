use std::fs;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use sysinfo::{Components, Disks, Networks, System};

use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, State,
};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AppSettings {
    pub show_cpu: bool,
    pub show_ram: bool,
    pub show_swap: bool,
    pub show_load: bool,
    pub show_disk: bool,
    pub show_net: bool,
    pub show_temp: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        let is_macos = cfg!(target_os = "macos");
        Self {
            show_cpu: true,
            show_ram: true,
            show_swap: !is_macos,
            show_load: !is_macos,
            show_disk: !is_macos,
            show_net: !is_macos,
            show_temp: true,
        }
    }
}

pub struct SettingsState(Arc<Mutex<AppSettings>>);

fn get_config_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|p| p.join("settings.json"))
}

fn load_settings(app: &AppHandle) -> AppSettings {
    if let Some(path) = get_config_path(app) {
        match fs::read_to_string(path) {
            Ok(data) => match serde_json::from_str(&data) {
                Ok(settings) => return settings,
                Err(e) => eprintln!("Failed to parse settings: {e}"),
            },
            Err(e) => eprintln!("Failed to read settings: {e}"),
        }
    }
    AppSettings::default()
}

fn save_settings(app: &AppHandle, settings: &AppSettings) {
    if let Some(path) = get_config_path(app) {
        if let Some(parent) = path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                eprintln!("Failed to create config directory: {e}");
                return;
            }
        }

        match serde_json::to_string_pretty(settings) {
            Ok(data) => {
                if let Err(e) = fs::write(path, data) {
                    eprintln!("Failed to write settings: {e}");
                }
            }
            Err(e) => eprintln!("Failed to serialize settings: {e}"),
        }
    }
}

#[tauri::command]
fn get_settings(state: State<SettingsState>) -> AppSettings {
    state.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
fn update_settings(app: AppHandle, settings: AppSettings, state: State<SettingsState>) {
    if let Ok(mut s) = state.0.lock() {
        *s = settings.clone();
    }
    save_settings(&app, &settings);
}

fn fmt_rate(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1}M", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.0}K", bytes as f64 / 1_000.0)
    } else {
        format!("{}B", bytes)
    }
}

fn cpu_temp(components: &Components) -> Option<f32> {
    let candidates = ["package", "tctl", "cpu", "core 0", "core"];

    for c in components.iter() {
        let label = c.label().to_lowercase();
        for keyword in &candidates {
            if label.contains(keyword) {
                if let Some(t) = c.temperature() {
                    return Some(t);
                }
            }
        }
    }

    components
        .iter()
        .find_map(|c| c.temperature().filter(|&t| t > 0.0))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let settings = Arc::new(Mutex::new(AppSettings::default()));
    let settings_thread = settings.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(SettingsState(settings))
        .invoke_handler(tauri::generate_handler![get_settings, update_settings])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .setup(move |app| {
            let loaded = load_settings(app.handle());
            let state = app.state::<SettingsState>();
            if let Ok(mut s) = state.0.lock() {
                *s = loaded;
            }

            let login_i = CheckMenuItem::with_id(
                app,
                "start_on_login",
                "Start on Login",
                true,
                false,
                None::<&str>,
            )?;

            let settings_i =
                MenuItem::with_id(app, "settings", "Preferences...", true, None::<&str>)?;

            let sep = PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

            let menu = Menu::with_items(app, &[&login_i, &settings_i, &sep, &quit_i])?;

            let tray = TrayIconBuilder::with_id("main-tray")
                .tooltip("sys_monitor")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "settings" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            let tray_c = tray.clone();

            thread::spawn(move || {
                let mut sys = System::new_all();
                let mut nets = Networks::new_with_refreshed_list();
                let mut disks = Disks::new_with_refreshed_list();
                let mut components = Components::new_with_refreshed_list();

                sys.refresh_cpu_usage();
                thread::sleep(Duration::from_millis(500));

                let mut prev_rx: u64 =
                    nets.values().map(|n| n.total_received()).sum();
                let mut prev_tx: u64 =
                    nets.values().map(|n| n.total_transmitted()).sum();

                loop {
                    sys.refresh_cpu_usage();
                    sys.refresh_memory();
                    nets.refresh(true);
                    disks.refresh(false);
                    components.refresh(false);

                    let cpu = sys.global_cpu_usage();
                    let temp = cpu_temp(&components);

                    let ram = {
                        let total = sys.total_memory();
                        if total > 0 {
                            (sys.used_memory() as f64 / total as f64 * 100.0) as f32
                        } else {
                            0.0
                        }
                    };

                    let swap = {
                        let total = sys.total_swap();
                        if total > 0 {
                            (sys.used_swap() as f64 / total as f64 * 100.0) as f32
                        } else {
                            0.0
                        }
                    };

                    let load = System::load_average().one;

                    let disk_total: u64 =
                        disks.iter().map(|d| d.total_space()).sum();
                    let disk_avail: u64 =
                        disks.iter().map(|d| d.available_space()).sum();

                    let disk_pct = if disk_total > 0 {
                        ((disk_total - disk_avail) as f64 / disk_total as f64 * 100.0) as f32
                    } else {
                        0.0
                    };

                    let cur_rx: u64 =
                        nets.values().map(|n| n.total_received()).sum();
                    let cur_tx: u64 =
                        nets.values().map(|n| n.total_transmitted()).sum();

                    let dl = cur_rx.saturating_sub(prev_rx);
                    let ul = cur_tx.saturating_sub(prev_tx);

                    prev_rx = cur_rx;
                    prev_tx = cur_tx;

                    let settings = match settings_thread.lock() {
                        Ok(s) => s.clone(),
                        Err(poisoned) => poisoned.into_inner().clone(),
                    };

                    let mut parts = Vec::new();

                    if settings.show_cpu {
                        if settings.show_temp {
                            if let Some(t) = temp {
                                parts.push(format!("CPU {:.0}% {:.0}°C", cpu, t));
                            } else {
                                parts.push(format!("CPU {:.0}%", cpu));
                            }
                        } else {
                            parts.push(format!("CPU {:.0}%", cpu));
                        }
                    }

                    if settings.show_ram {
                        parts.push(format!("RAM {:.0}%", ram));
                    }

                    if settings.show_swap {
                        parts.push(format!("Swap {:.0}%", swap));
                    }

                    if settings.show_load {
                        parts.push(format!("Load {:.1}", load));
                    }

                    if settings.show_disk {
                        parts.push(format!("Disk {:.0}%", disk_pct));
                    }

                    if settings.show_net {
                        parts.push(format!("↓{} ↑{}", fmt_rate(dl), fmt_rate(ul)));
                    }

                    let label = parts.join(" | ");

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
