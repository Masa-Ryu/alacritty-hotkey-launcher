use crate::common_backend::AppConfig;
use rdev::Key;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct FileConfig {
    #[serde(default)]
    settings: Settings,
}

#[derive(Debug, Deserialize)]
struct Settings {
    #[serde(default = "default_interval")] 
    interval: u64,
    #[serde(default = "default_app_path")] 
    app_path: String,
    #[serde(default = "default_app_name")] 
    app_name: String,
    // Accept either a single key or the first of an array named detected_keys
    #[serde(default)]
    detected_key: Option<String>,
    #[serde(default)]
    detected_keys: Option<Vec<String>>,
}

fn default_interval() -> u64 { 300 }
fn default_app_path() -> String { "/usr/local/bin/alacritty".to_string() }
fn default_app_name() -> String { "Alacritty".to_string() }

impl Default for Settings {
    fn default() -> Self {
        Self {
            interval: default_interval(),
            app_path: default_app_path(),
            app_name: default_app_name(),
            detected_key: None,
            detected_keys: None,
        }
    }
}

pub fn load_from_str(s: &str) -> AppConfig {
    // Allow both [settings] and legacy [settigs]. If legacy header exists and no
    // proper [settings] header, prefer the legacy-rewritten version.
    let parsed: Option<FileConfig> = if s.contains("[settigs]") && !s.contains("[settings]") {
        let fixed = s.replace("[settigs]", "[settings]");
        toml::from_str::<FileConfig>(&fixed).ok()
    } else {
        toml::from_str::<FileConfig>(s).ok()
    };

    let settings = parsed.map(|f| f.settings).unwrap_or_default();
    let (interval, app_path, app_name) = (settings.interval, settings.app_path, settings.app_name);

    // Determine key: prefer detected_key, else first of detected_keys
    let key_str = settings
        .detected_key
        .or_else(|| settings.detected_keys.and_then(|v| v.into_iter().next()))
        .unwrap_or_else(|| "ctrl_left".to_string());

    let detect_key = parse_key(&key_str).unwrap_or(Key::ControlLeft);

    AppConfig {
        double_press_interval: Duration::from_millis(interval),
        app_path,
        app_name,
        detect_key,
    }
}

pub fn load_from_file(path: impl AsRef<Path>) -> Option<AppConfig> {
    let p = path.as_ref();
    let content = fs::read_to_string(p).ok()?;
    Some(load_from_str(&content))
}

// Minimal parser for common key names. Case-insensitive.
fn parse_key(s: &str) -> Option<Key> {
    let k = s.to_ascii_lowercase();
    match k.as_str() {
        "ctrl" | "control" | "ctrl_left" | "control_left" | "left_ctrl" | "left_control" | "controlleft" => Some(Key::ControlLeft),
        "ctrl_right" | "control_right" | "right_ctrl" | "right_control" | "controlright" => Some(Key::ControlRight),
        // Extend as needed
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_config_ok() {
        let s = r#"
            [settings]
            interval = 450
            app_path = "/bin/echo"
            app_name = "Echo"
            detected_key = "ctrl_left"
        "#;
        let cfg = load_from_str(s);
        assert_eq!(cfg.double_press_interval, Duration::from_millis(450));
        assert_eq!(cfg.app_path, "/bin/echo");
        assert_eq!(cfg.app_name, "Echo");
        assert!(matches!(cfg.detect_key, Key::ControlLeft));
    }

    #[test]
    fn parse_legacy_table_and_array_key() {
        let s = r#"
            [settigs]
            interval = 300
            app_path = "/usr/local/bin/alacritty"
            app_name = "alacritty"
            detected_keys = ["CTRL_LEFT", "CTRL_RIGHT"]
        "#;
        let cfg = load_from_str(s);
        assert_eq!(cfg.double_press_interval, Duration::from_millis(300));
        assert_eq!(cfg.app_name, "alacritty");
        assert!(matches!(cfg.detect_key, Key::ControlLeft));
    }

    #[test]
    fn parse_defaults_on_missing_or_invalid_key() {
        let s = r#"
            [settings]
            app_path = "/bin/echo"
            app_name = "Echo"
            detected_key = "unknown_key"
        "#;
        let cfg = load_from_str(s);
        // interval default 300
        assert_eq!(cfg.double_press_interval, Duration::from_millis(300));
        // invalid key -> default ControlLeft
        assert!(matches!(cfg.detect_key, Key::ControlLeft));
    }
}
