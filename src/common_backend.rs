use rdev::Key;
use std::time::{Duration, Instant};

// Public config shared by backends and orchestrator
pub struct AppConfig {
    pub double_press_interval: Duration,
    pub app_path: String,
    pub app_name: String,
    pub detect_key: Key,
}

// Unified backend interface. Uses a portable u64 as WindowId.
pub trait WindowBackend {
    fn find_window(&mut self, app_name: &str) -> Option<u64>;
    fn is_on_current_workspace(&mut self, window: u64) -> bool;
    fn is_visible(&mut self, window: u64) -> bool;
    fn move_to_current_workspace(&mut self, window: u64);
    fn show(&mut self, window: u64);
    fn hide(&mut self, window: u64);
    fn launch_app(&mut self, app_path: &str);
}

// Core orchestration logic, backend-agnostic.
pub fn toggle_or_launch(backend: &mut dyn WindowBackend, cfg: &AppConfig) {
    if let Some(id) = backend.find_window(&cfg.app_name) {
        if backend.is_on_current_workspace(id) {
            if backend.is_visible(id) {
                backend.hide(id);
            } else {
                backend.show(id);
            }
        } else {
            backend.move_to_current_workspace(id);
            backend.show(id);
        }
    } else {
        backend.launch_app(&cfg.app_path);
    }
}

// Robust double-press detector that requires a release between presses.
pub struct DoublePressDetector {
    interval: Duration,
    target: Key,
    last_press: Option<Instant>,
    saw_release_since_last_press: bool,
}

impl DoublePressDetector {
    pub fn new(interval: Duration, target: Key) -> Self {
        Self {
            interval,
            target,
            last_press: None,
            saw_release_since_last_press: false,
        }
    }

    pub fn on_key_press(&mut self, key: Key, now: Instant) -> bool {
        if key != self.target {
            return false;
        }

        let triggered = if let Some(prev) = self.last_press {
            self.saw_release_since_last_press && now.duration_since(prev) <= self.interval
        } else {
            false
        };

        if triggered {
            // Reset to avoid triple chaining
            self.last_press = None;
            self.saw_release_since_last_press = false;
            true
        } else {
            self.last_press = Some(now);
            self.saw_release_since_last_press = false;
            false
        }
    }

    pub fn on_key_release(&mut self, key: Key, _now: Instant) {
        if key == self.target {
            self.saw_release_since_last_press = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Simple in-memory mock backend for testing orchestrator
    struct MockBackend {
        has_window: bool,
        on_ws: bool,
        visible: bool,
        moved: bool,
        shown: bool,
        hidden: bool,
        launched: bool,
    }

    impl MockBackend {
        fn new(has_window: bool, on_ws: bool, visible: bool) -> Self {
            Self {
                has_window,
                on_ws,
                visible,
                moved: false,
                shown: false,
                hidden: false,
                launched: false,
            }
        }
    }

    impl WindowBackend for MockBackend {
        fn find_window(&mut self, _app_name: &str) -> Option<u64> {
            if self.has_window {
                Some(1)
            } else {
                None
            }
        }
        fn is_on_current_workspace(&mut self, _window: u64) -> bool {
            self.on_ws
        }
        fn is_visible(&mut self, _window: u64) -> bool {
            self.visible
        }
        fn move_to_current_workspace(&mut self, _window: u64) {
            self.moved = true;
            self.on_ws = true;
        }
        fn show(&mut self, _window: u64) {
            self.shown = true;
            self.visible = true;
        }
        fn hide(&mut self, _window: u64) {
            self.hidden = true;
            self.visible = false;
        }
        fn launch_app(&mut self, _app_path: &str) {
            self.launched = true;
            self.has_window = true;
        }
    }

    #[test]
    fn orchestrator_hides_when_visible_on_ws() {
        let mut be = MockBackend::new(true, true, true);
        let cfg = AppConfig {
            double_press_interval: Duration::from_millis(300),
            app_path: "test".into(),
            app_name: "Alacritty".into(),
            detect_key: Key::ControlLeft,
        };
        toggle_or_launch(&mut be, &cfg);
        assert!(be.hidden);
        assert!(!be.shown);
        assert!(!be.launched);
    }

    #[test]
    fn orchestrator_shows_when_hidden_on_ws() {
        let mut be = MockBackend::new(true, true, false);
        let cfg = AppConfig {
            double_press_interval: Duration::from_millis(300),
            app_path: "test".into(),
            app_name: "Alacritty".into(),
            detect_key: Key::ControlLeft,
        };
        toggle_or_launch(&mut be, &cfg);
        assert!(be.shown);
        assert!(!be.hidden);
        assert!(!be.launched);
    }

    #[test]
    fn orchestrator_moves_and_shows_when_on_other_ws() {
        let mut be = MockBackend::new(true, false, false);
        let cfg = AppConfig {
            double_press_interval: Duration::from_millis(300),
            app_path: "test".into(),
            app_name: "Alacritty".into(),
            detect_key: Key::ControlLeft,
        };
        toggle_or_launch(&mut be, &cfg);
        assert!(be.moved);
        assert!(be.shown);
        assert!(!be.hidden);
        assert!(!be.launched);
    }

    #[test]
    fn orchestrator_launches_when_not_found() {
        let mut be = MockBackend::new(false, false, false);
        let cfg = AppConfig {
            double_press_interval: Duration::from_millis(300),
            app_path: "test".into(),
            app_name: "Alacritty".into(),
            detect_key: Key::ControlLeft,
        };
        toggle_or_launch(&mut be, &cfg);
        assert!(be.launched);
    }

    #[test]
    fn double_press_requires_release_and_interval() {
        let target = Key::ControlLeft;
        let mut dp = DoublePressDetector::new(Duration::from_millis(250), target);
        let t0 = Instant::now();
        assert!(!dp.on_key_press(target, t0));
        // Without release, second press should not trigger
        assert!(!dp.on_key_press(target, t0 + Duration::from_millis(100)));
        // Now release, then press quickly should trigger
        dp.on_key_release(target, t0 + Duration::from_millis(110));
        assert!(dp.on_key_press(target, t0 + Duration::from_millis(200)));
        // After trigger, next press starts new sequence
        assert!(!dp.on_key_press(target, t0 + Duration::from_millis(300)));
        dp.on_key_release(target, t0 + Duration::from_millis(310));
        // Too late
        assert!(!dp.on_key_press(target, t0 + Duration::from_millis(700)));
    }
}
