import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

interface AppSettings {
  show_cpu: boolean;
  show_ram: boolean;
  show_swap: boolean;
  show_load: boolean;
  show_disk: boolean;
  show_net: boolean;
  show_temp: boolean;
}

function App() {
  const [settings, setSettings] = useState<AppSettings | null>(null);

  useEffect(() => {
    invoke<AppSettings>("get_settings").then(setSettings).catch(console.error);
  }, []);

  const handleChange = (key: keyof AppSettings) => {
    if (!settings) return;
    const newSettings = { ...settings, [key]: !settings[key] };
    setSettings(newSettings);
    invoke("update_settings", { settings: newSettings }).catch(console.error);
  };

  if (!settings) return <div className="container">Loading...</div>;

  return (
    <div className="container">
      <h1>Preferences</h1>
      <div className="settings-list">
        <label>
          <input
            type="checkbox"
            checked={settings.show_cpu}
            onChange={() => handleChange("show_cpu")}
          />
          Show CPU Usage
        </label>
        <label className="sub-setting" style={{ marginLeft: "20px", display: "block" }}>
            <input
              type="checkbox"
              checked={settings.show_temp}
              disabled={!settings.show_cpu}
              onChange={() => handleChange("show_temp")}
            />
            Show CPU Temperature
        </label>
        <label>
          <input
            type="checkbox"
            checked={settings.show_ram}
            onChange={() => handleChange("show_ram")}
          />
          Show RAM Usage
        </label>
        <label>
          <input
            type="checkbox"
            checked={settings.show_swap}
            onChange={() => handleChange("show_swap")}
          />
          Show Swap Usage
        </label>
        <label>
          <input
            type="checkbox"
            checked={settings.show_load}
            onChange={() => handleChange("show_load")}
          />
          Show Load Average
        </label>
        <label>
          <input
            type="checkbox"
            checked={settings.show_disk}
            onChange={() => handleChange("show_disk")}
          />
          Show Disk Usage
        </label>
        <label>
          <input
            type="checkbox"
            checked={settings.show_net}
            onChange={() => handleChange("show_net")}
          />
          Show Network Speed
        </label>
      </div>
    </div>
  );
}

export default App;
