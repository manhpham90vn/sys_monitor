use std::fmt::Write;
use sysinfo::{Components, Disks, Networks, System};

/// Tick counter thresholds for tiered refresh intervals.
/// Fast metrics (CPU, RAM, Network) refresh every tick (1s).
/// Medium metrics (Swap, Load, Temperature) refresh every 5 ticks (5s).
/// Slow metrics (Disk) refresh every 30 ticks (30s).
const MEDIUM_INTERVAL: u64 = 5;
const SLOW_INTERVAL: u64 = 30;

/// Holds all system metric subsystems and cached values.
pub struct SystemMetrics {
    sys: System,
    nets: Networks,
    disks: Disks,
    components: Components,

    // Cached metric values
    pub cpu: f32,
    pub temp: Option<f32>,
    pub ram: f32,
    pub swap: f32,
    pub load: f64,
    pub disk_pct: f32,
    pub dl: u64,
    pub ul: u64,

    // Network byte tracking for speed calculation
    prev_rx: u64,
    prev_tx: u64,

    // Tick counter for tiered refresh
    tick: u64,

    // Reusable label buffer to avoid allocations each tick
    label_buf: String,
}

impl SystemMetrics {
    /// Creates a new `SystemMetrics` instance with minimal initialization.
    /// Uses `System::new()` instead of `System::new_all()` to avoid loading
    /// unnecessary data (processes, users, etc.).
    pub fn new() -> Self {
        let mut sys = System::new();
        let nets = Networks::new_with_refreshed_list();
        let disks = Disks::new_with_refreshed_list();
        let components = Components::new_with_refreshed_list();

        // sysinfo requires two consecutive CPU usage refreshes for meaningful delta
        sys.refresh_cpu_usage();

        let prev_rx: u64 = nets.values().map(|n| n.total_received()).sum();
        let prev_tx: u64 = nets.values().map(|n| n.total_transmitted()).sum();

        // Pre-compute initial disk percentage
        let disk_total: u64 = disks.iter().map(|d| d.total_space()).sum();
        let disk_avail: u64 = disks.iter().map(|d| d.available_space()).sum();
        let disk_pct = if disk_total > 0 {
            ((disk_total - disk_avail) as f64 / disk_total as f64 * 100.0) as f32
        } else {
            0.0
        };

        Self {
            sys,
            nets,
            disks,
            components,
            cpu: 0.0,
            temp: None,
            ram: 0.0,
            swap: 0.0,
            load: 0.0,
            disk_pct,
            dl: 0,
            ul: 0,
            prev_rx,
            prev_tx,
            tick: 0,
            label_buf: String::with_capacity(128),
        }
    }

    /// Refreshes metrics with tiered intervals:
    /// - Every 1s:  CPU, RAM, Network
    /// - Every 5s:  Swap, Load average, Temperature
    /// - Every 30s: Disk usage
    pub fn refresh(&mut self) {
        // ── Fast metrics (every tick) ────────────────────────────
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.nets.refresh(false);

        self.cpu = self.sys.global_cpu_usage();

        let total_mem = self.sys.total_memory();
        self.ram = if total_mem > 0 {
            (self.sys.used_memory() as f64 / total_mem as f64 * 100.0) as f32
        } else {
            0.0
        };

        // Network throughput (bytes/sec delta)
        let cur_rx: u64 = self.nets.values().map(|n| n.total_received()).sum();
        let cur_tx: u64 = self.nets.values().map(|n| n.total_transmitted()).sum();
        self.dl = cur_rx.saturating_sub(self.prev_rx);
        self.ul = cur_tx.saturating_sub(self.prev_tx);
        self.prev_rx = cur_rx;
        self.prev_tx = cur_tx;

        // ── Medium metrics (every 5s) ────────────────────────────
        if self.tick.is_multiple_of(MEDIUM_INTERVAL) {
            let total_swap = self.sys.total_swap();
            self.swap = if total_swap > 0 {
                (self.sys.used_swap() as f64 / total_swap as f64 * 100.0) as f32
            } else {
                0.0
            };

            self.load = System::load_average().one;

            self.components.refresh(false);
            self.temp = cpu_temp(&self.components);
        }

        // ── Slow metrics (every 30s) ─────────────────────────────
        if self.tick.is_multiple_of(SLOW_INTERVAL) {
            self.disks.refresh(false);
            let disk_total: u64 = self.disks.iter().map(|d| d.total_space()).sum();
            let disk_avail: u64 = self.disks.iter().map(|d| d.available_space()).sum();
            self.disk_pct = if disk_total > 0 {
                ((disk_total - disk_avail) as f64 / disk_total as f64 * 100.0) as f32
            } else {
                0.0
            };
        }

        self.tick = self.tick.wrapping_add(1);
    }

    /// Formats the current metrics into a compact panel label string.
    /// Reuses an internal buffer to avoid allocating a new String each tick.
    pub fn format_label(&mut self) -> &str {
        self.label_buf.clear();

        let _ = write!(self.label_buf, "CPU {:.0}%", self.cpu);

        if let Some(t) = self.temp {
            let _ = write!(self.label_buf, " {:.0}°C", t);
        }

        let _ = write!(
            self.label_buf,
            " | RAM {:.0}% | Swap {:.0}% | Load Average {:.1} | Disk {:.0}% | Net ↓{} ↑{}",
            self.ram,
            self.swap,
            self.load,
            self.disk_pct,
            fmt_rate(self.dl),
            fmt_rate(self.ul)
        );

        &self.label_buf
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fmt_rate_bytes() {
        assert_eq!(fmt_rate(42), "42B");
        assert_eq!(fmt_rate(0), "0B");
        assert_eq!(fmt_rate(999), "999B");
    }

    #[test]
    fn test_fmt_rate_kilobytes() {
        assert_eq!(fmt_rate(1_000), "1K");
        assert_eq!(fmt_rate(1_500), "2K");
        assert_eq!(fmt_rate(300_000), "300K");
    }

    #[test]
    fn test_fmt_rate_megabytes() {
        assert_eq!(fmt_rate(1_000_000), "1.0M");
        assert_eq!(fmt_rate(1_200_000), "1.2M");
        assert_eq!(fmt_rate(10_500_000), "10.5M");
    }

    #[test]
    fn test_metrics_new() {
        let metrics = SystemMetrics::new();
        assert_eq!(metrics.cpu, 0.0);
        assert_eq!(metrics.ram, 0.0);
        assert_eq!(metrics.tick, 0);
    }

    #[test]
    fn test_format_label_without_temp() {
        let mut metrics = SystemMetrics::new();
        metrics.cpu = 25.0;
        metrics.ram = 60.0;
        metrics.swap = 10.0;
        metrics.load = 1.5;
        metrics.disk_pct = 45.0;
        metrics.dl = 1_500;
        metrics.ul = 500;
        metrics.temp = None;

        let label = metrics.format_label();
        assert!(label.starts_with("CPU 25%"));
        assert!(label.contains("RAM 60%"));
        assert!(label.contains("Swap 10%"));
        assert!(label.contains("Load Average 1.5"));
        assert!(label.contains("Disk 45%"));
        assert!(label.contains("↓2K"));
        assert!(label.contains("↑500B"));
        assert!(!label.contains("°C"));
    }

    #[test]
    fn test_format_label_with_temp() {
        let mut metrics = SystemMetrics::new();
        metrics.cpu = 50.0;
        metrics.temp = Some(65.0);

        let label = metrics.format_label();
        assert!(label.contains("CPU 50% 65°C"));
    }
}
