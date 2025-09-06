use crate::common_backend::WindowBackend;
use std::process::Command;

// Wayland is compositor-specific for global window control.
// This backend acts conservatively: we cannot reliably find/toggle windows
// without compositor protocols (e.g., GNOME extension, sway IPC, hyprctl).
// Therefore, we fallback to launching when requested and report no window found.
pub struct WaylandBackend;

impl WaylandBackend {
    pub fn new() -> Self { Self }
}

impl WindowBackend for WaylandBackend {
    fn find_window(&mut self, _app_name: &str) -> Option<u64> { None }
    fn is_on_current_workspace(&mut self, _window: u64) -> bool { false }
    fn is_visible(&mut self, _window: u64) -> bool { false }
    fn move_to_current_workspace(&mut self, _window: u64) { /* no-op */ }
    fn show(&mut self, _window: u64) { /* no-op */ }
    fn hide(&mut self, _window: u64) { /* no-op */ }
    fn launch_app(&mut self, app_path: &str) {
        let _ = Command::new(app_path).spawn();
    }
}

