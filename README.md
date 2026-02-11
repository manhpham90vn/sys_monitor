# sys_monitor

A lightweight system monitor that displays **real-time metrics** directly on the GNOME top panel via **AppIndicator/System Tray**.

Built with **Tauri v2** + **Rust** (`sysinfo` crate). Tray-only — no dashboard window.

## Features

| Metric           | Details                                              |
| ---------------- | ---------------------------------------------------- |
| **CPU**          | Usage % + temperature (°C)                           |
| **RAM**          | Usage %                                              |
| **Swap**         | Usage %                                              |
| **Load Average** | 1-minute load average                                |
| **Disk**         | Usage % across all mount points                      |
| **Network**      | Download ↓ / Upload ↑ speed (auto-scaled B/s → MB/s) |

- 🔄 Updates every **1 second**
- 📌 Metrics displayed as text directly on the **GNOME top bar**
- 🖱️ Right-click menu: Start on Login (placeholder) + Quit
- 🪶 Lightweight, tray-only — no dashboard window

## System Requirements

- **Ubuntu 24.04** (GNOME desktop)
- **AppIndicator** extension (pre-installed on Ubuntu as `ubuntu-appindicators`)

```bash
# Verify the extension is installed
gnome-extensions list | grep appindicator

# Install if missing
sudo apt install gnome-shell-extension-appindicator
# Log out and log back in to activate
```

## Install from .deb

Download the `.deb` from [GitHub Releases](../../releases), then:

```bash
sudo dpkg -i sys-monitor_*.deb
```

## Development

### Prerequisites

```bash
# System libraries (Ubuntu/Debian)
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf libgtk-3-dev libsoup-3.0-dev \
  javascriptcoregtk-4.1 libglib2.0-dev

# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Node.js dependencies
npm install
```

### Run in dev mode

```bash
TAURI_LINUX_AYATANA_APPINDICATOR=1 npm run tauri dev
```

### Build .deb package

```bash
npm run tauri build
# Output: src-tauri/target/release/bundle/deb/sys-monitor_*.deb
```

### Generate custom icon

```bash
# From a 1024x1024 PNG source
npx tauri icon path/to/app-icon.png
```

## CI/CD

Automated via GitHub Actions:

| Trigger              | Action                                    |
| -------------------- | ----------------------------------------- |
| Push / PR → `master` | Build + lint + upload `.deb` artifact     |
| Push tag `v*`        | Build + create GitHub Release with `.deb` |

```bash
# Create a release
git tag v0.1.0
git push --tags
```

## Project Structure

```
.
├── src/                        # Frontend (React, minimal — tray-only app)
│   ├── App.tsx
│   └── App.css
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs              # Core logic: tray setup + sysinfo metrics loop
│   │   └── main.rs             # Binary entry point
│   ├── Cargo.toml              # Rust dependencies (tauri, sysinfo)
│   ├── tauri.conf.json         # Tauri config, window, .deb bundle settings
│   └── capabilities/           # Tauri permission declarations
├── .github/workflows/
│   └── build.yml               # CI/CD pipeline
└── package.json
```

## Troubleshooting

### Tray icon not visible on GNOME

```bash
sudo apt install gnome-shell-extension-appindicator
gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com
# Log out and log back in
```

### No text on the top panel

Make sure the `TAURI_LINUX_AYATANA_APPINDICATOR` environment variable is set:

```bash
TAURI_LINUX_AYATANA_APPINDICATOR=1 npm run tauri dev
```

### CPU temperature shows nothing

```bash
sudo apt install lm-sensors
sudo sensors-detect --auto
```

### Build fails with missing libraries

```bash
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
```

## Tech Stack

- **[Tauri v2](https://v2.tauri.app/)** — cross-platform app framework
- **[sysinfo](https://crates.io/crates/sysinfo)** — system metrics collection
- **React + Vite** — frontend (minimal, hidden window)
- **AppIndicator** — GNOME system tray integration

## License

MIT
