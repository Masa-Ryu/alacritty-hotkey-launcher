mod common_backend;
mod config;
mod x11_ewmh;
mod x11_backend;
mod wayland_backend;

use common_backend::{toggle_or_launch, AppConfig, DoublePressDetector};
use rdev::{listen, Event, EventType, Key};
use std::env;
use std::time::{Duration, Instant};

fn main() {
    println!("Hotkey listener started");

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
        BackendKind::Wayland => Box::new(wayland_backend::WaylandBackend::new()),
    };

    // Load config from file if present; fall back to defaults
    let config_path = env::var("ALACRITTY_HOTKEY_LAUNCHER_CONFIG")
        .ok()
        .unwrap_or_else(|| "src/config.toml".to_string());
    let config = config::load_from_file(&config_path)
        .unwrap_or_else(|| AppConfig {
            double_press_interval: Duration::from_millis(300),
            app_path: "/usr/local/bin/alacritty".to_string(),
            app_name: "Alacritty".to_string(),
            detect_key: Key::ControlLeft,
        });

    let mut detector = DoublePressDetector::new(config.double_press_interval, config.detect_key);

    if let Err(error) = listen(move |event| handle_event(event, &mut detector, &mut *backend, &config)) {
        eprintln!("Error: {:?}", error);
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
