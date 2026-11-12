//! Live readings for Home, the dashboard and the quick panel: temperatures, power, battery and fans, from the
//! kernel's hwmon and power_supply files (readable without a password).

use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Readings {
    pub cpu_c: Option<f64>,
    pub gpu_c: Option<f64>,
    /// Graphics power draw (the whole APU on a handheld).
    pub gpu_w: Option<f64>,
    pub battery_pct: Option<u32>,
    /// Battery draw while discharging, in watts.
    pub battery_w: Option<f64>,
    pub charging: bool,
    pub fan_rpm: Option<u32>,
}

fn text(p: &Path) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

fn num(p: &Path) -> Option<f64> {
    text(p)?.parse().ok()
}

/// The temperature (°C) whose label is one of `labels` in this hwmon folder, else temp1.
fn temp(dir: &Path, labels: &[&str]) -> Option<f64> {
    let names: Vec<String> = fs::read_dir(dir).ok()?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    for want in labels {
        for n in names.iter().filter(|n| n.starts_with("temp") && n.ends_with("_label")) {
            if text(&dir.join(n)).as_deref() == Some(*want) {
                return num(&dir.join(n.replace("_label", "_input"))).map(|v| v / 1000.0);
            }
        }
    }
    num(&dir.join("temp1_input")).map(|v| v / 1000.0)
}

pub fn read(root: &Path) -> Readings {
    let mut r = Readings::default();
    let hw = root.join("sys/class/hwmon");
    let mut dirs: Vec<_> = fs::read_dir(&hw).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    dirs.sort();
    for d in dirs {
        match text(&d.join("name")).as_deref() {
            Some("k10temp") => r.cpu_c = r.cpu_c.or(temp(&d, &["Tctl", "Tdie"])),
            Some("coretemp") => r.cpu_c = r.cpu_c.or(temp(&d, &["Package id 0"])),
            Some("amdgpu") => {
                r.gpu_c = r.gpu_c.or(temp(&d, &["edge", "junction"]));
                r.gpu_w = r.gpu_w.or(num(&d.join("power1_average")).or_else(|| num(&d.join("power1_input"))).map(|v| v / 1_000_000.0));
                r.fan_rpm = r.fan_rpm.or(num(&d.join("fan1_input")).map(|v| v as u32));
            }
            Some(n) if n.ends_with("_fan") || n == "asus" || n == "steamdeck_hwmon" => r.fan_rpm = r.fan_rpm.or(num(&d.join("fan1_input")).map(|v| v as u32)),
            _ => {}
        }
    }
    let ps = root.join("sys/class/power_supply");
    let mut supplies: Vec<_> = fs::read_dir(&ps).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    supplies.sort();
    if let Some(b) = supplies.iter().find(|b| text(&b.join("type")).as_deref() == Some("Battery") && text(&b.join("scope")).as_deref() != Some("Device")) {
        r.battery_pct = num(&b.join("capacity")).map(|v| v as u32);
        let status = text(&b.join("status")).unwrap_or_default();
        r.charging = status == "Charging" || status == "Full";
        let watts = num(&b.join("power_now")).or_else(|| Some(num(&b.join("current_now"))? * num(&b.join("voltage_now"))? / 1_000_000.0)).map(|v| v / 1_000_000.0);
        r.battery_w = if r.charging { None } else { watts };
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(root: &Path, p: &str, t: &str) {
        let f = root.join(p);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, t).unwrap();
    }

    #[test]
    fn reads_temps_power_battery_and_fans() {
        let root = std::env::temp_dir().join(format!("turbinehub-sensors-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        w(&root, "sys/class/hwmon/hwmon5/name", "coretemp\n");
        w(&root, "sys/class/hwmon/hwmon5/temp1_label", "Package id 0\n");
        w(&root, "sys/class/hwmon/hwmon5/temp1_input", "41000\n");
        w(&root, "sys/class/hwmon/hwmon5/temp2_label", "Core 0\n");
        w(&root, "sys/class/hwmon/hwmon5/temp2_input", "39000\n");
        w(&root, "sys/class/hwmon/hwmon6/name", "amdgpu\n");
        w(&root, "sys/class/hwmon/hwmon6/temp1_label", "edge\n");
        w(&root, "sys/class/hwmon/hwmon6/temp1_input", "38000\n");
        w(&root, "sys/class/hwmon/hwmon6/power1_average", "34000000\n");
        w(&root, "sys/class/hwmon/hwmon6/fan1_input", "0\n");
        w(&root, "sys/class/power_supply/BAT0/type", "Battery\n");
        w(&root, "sys/class/power_supply/BAT0/capacity", "78\n");
        w(&root, "sys/class/power_supply/BAT0/status", "Discharging\n");
        w(&root, "sys/class/power_supply/BAT0/power_now", "14200000\n");
        let r = read(&root);
        assert_eq!(r, Readings { cpu_c: Some(41.0), gpu_c: Some(38.0), gpu_w: Some(34.0), battery_pct: Some(78), battery_w: Some(14.2), charging: false, fan_rpm: Some(0) });
        w(&root, "sys/class/power_supply/BAT0/status", "Charging\n");
        assert_eq!(read(&root).battery_w, None);
        let _ = fs::remove_dir_all(&root);
        assert_eq!(read(Path::new("/nonexistent")), Readings::default());
    }
}
