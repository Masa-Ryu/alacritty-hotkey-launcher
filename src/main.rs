mod common_backend;
mod config;
mod wayland_backend;
mod x11_backend;
mod x11_ewmh;

use common_backend::{toggle_or_launch, AppConfig, DoublePressDetector};
use rdev::{listen, Event, EventType, Key};
use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() {
    println!("Hotkey listener started");

    // Load config with precedence:
    // 1) ALACRITTY_HOTKEY_LAUNCHER_CONFIG
    // 2) ~/.config/alacritty-hotkey-launcher/config.toml
    // 3) src/config.toml
    let config = load_app_config();

    // Decide backend: prefer X11 if DISPLAY is available (works under Xwayland too)
    let backend_kind = if env::var_os("DISPLAY").is_some() {
        BackendKind::X11
    } else if env::var_os("WAYLAND_DISPLAY").is_some() {
        BackendKind::Wayland
    } else {
        BackendKind::X11 // default fallback
    };

    let mut backend: Box<dyn common_backend::WindowBackend> = match backend_kind {
        BackendKind::X11 => Box::new(x11_backend::X11Backend::new()),
        BackendKind::Wayland => Box::new(wayland_backend::WaylandBackend::new_with_config(&config)),
    };

    let mut detector = DoublePressDetector::new(config.double_press_interval, config.detect_key);

    if let Err(error) =
        listen(move |event| handle_event(event, &mut detector, &mut *backend, &config))
    {
        eprintln!("Error: {:?}", error);
    }
}

fn load_app_config() -> AppConfig {
    // 1) Explicit override
    if let Ok(p) = env::var("ALACRITTY_HOTKEY_LAUNCHER_CONFIG") {
        if let Some(cfg) = config::load_from_file(&p) {
            return cfg;
        }
    }
    // 2) XDG-like default in home
    if let Some(home) = env::var_os("HOME") {
        let mut p = PathBuf::from(home);
        p.push(".config/alacritty-hotkey-launcher/config.toml");
        if let Some(cfg) = config::load_from_file(&p) {
            return cfg;
        }
    }
    // 3) repo default
    if let Some(cfg) = config::load_from_file("src/config.toml") {
        return cfg;
    }
    // Final fallback defaults
    AppConfig {
        double_press_interval: Duration::from_millis(300),
        app_path: "/usr/local/bin/alacritty".to_string(),
        app_name: "class=Alacritty".to_string(),
        detect_key: Key::ControlLeft,
        wayland_hide_method: common_backend::WaylandHideMethod::Auto,
    }
}

#[cfg(test)]
mod tests_load {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    fn env_lock<'a>() -> std::sync::MutexGuard<'a, ()> {
        ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn write_cfg(path: &PathBuf, interval: u64) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(path).unwrap();
        writeln!(f, "[settings]\ninterval = {}\napp_path = \"/bin/echo\"\napp_name = \"Echo\"\ndetected_key = \"ctrl_left\"", interval).unwrap();
    }

    #[test]
    fn load_config_prefers_env_override() {
        let _g = env_lock();
        let dir = tempdir().unwrap();
        let cfg_path = dir.path().join("override.toml");
        write_cfg(&cfg_path, 777);
        let old_env = env::var_os("ALACRITTY_HOTKEY_LAUNCHER_CONFIG");
        env::set_var("ALACRITTY_HOTKEY_LAUNCHER_CONFIG", &cfg_path);
        let cfg = load_app_config();
        assert_eq!(cfg.double_press_interval.as_millis(), 777);
        match old_env {
            Some(v) => env::set_var("ALACRITTY_HOTKEY_LAUNCHER_CONFIG", v),
            None => env::remove_var("ALACRITTY_HOTKEY_LAUNCHER_CONFIG"),
        }
    }

    #[test]
    fn load_config_prefers_home_when_no_env() {
        let _g = env_lock();
        let dir = tempdir().unwrap();
        let old_home = env::var_os("HOME");
        let old_env = env::var_os("ALACRITTY_HOTKEY_LAUNCHER_CONFIG");
        // fake HOME
        env::set_var("HOME", dir.path());
        env::remove_var("ALACRITTY_HOTKEY_LAUNCHER_CONFIG");
        let home_cfg = dir
            .path()
            .join(".config/alacritty-hotkey-launcher/config.toml");
        write_cfg(&home_cfg, 555);
        let cfg = load_app_config();
        assert_eq!(cfg.double_press_interval.as_millis(), 555);
        // restore envs
        match old_home {
            Some(v) => env::set_var("HOME", v),
            None => env::remove_var("HOME"),
        }
        match old_env {
            Some(v) => env::set_var("ALACRITTY_HOTKEY_LAUNCHER_CONFIG", v),
            None => env::remove_var("ALACRITTY_HOTKEY_LAUNCHER_CONFIG"),
        }
    }
}

#[derive(Copy, Clone)]
enum BackendKind {
    X11,
    Wayland,
}

fn handle_event(
    event: Event,
    detector: &mut DoublePressDetector,
    backend: &mut dyn common_backend::WindowBackend,
    config: &AppConfig,
) {
    match event.event_type {
        EventType::KeyPress(key) => {
            if detector.on_key_press(key, Instant::now()) {
                toggle_or_launch(backend, config);
            }
        }
        EventType::KeyRelease(key) => {
            detector.on_key_release(key, Instant::now());
        }
        _ => {}
    }
}
