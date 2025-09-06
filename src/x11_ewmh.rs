// EWMH helper utilities that are backend-agnostic enough to unit test.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientMessageSpec {
    pub message_type_atom: u64,
    pub window: u64,
    pub data: [i64; 5],
}

pub fn build_net_wm_desktop_message(window: u64, target_desktop: u64, net_wm_desktop_atom: u64) -> ClientMessageSpec {
    // data.l[0] = the new desktop number
    // data.l[1] = source indication (1 = application)
    ClientMessageSpec {
        message_type_atom: net_wm_desktop_atom,
        window,
        data: [target_desktop as i64, 1, 0, 0, 0],
    }
}

pub fn build_net_active_window_message(window: u64, net_active_window_atom: u64) -> ClientMessageSpec {
    // data.l[0] = source indication (1 = application)
    // data.l[1] = timestamp (CurrentTime = 0)
    // data.l[2] = currently active window and source (we use 0)
    ClientMessageSpec {
        message_type_atom: net_active_window_atom,
        window,
        data: [1, 0, 0, 0, 0],
    }
}

pub fn matches_app(target: &str, title: Option<&str>, wm_class: Option<&str>) -> bool {
    let t = target.trim();
    if t.is_empty() { return false; }

    // Optional explicit prefixes
    let (mode, pat) = if let Some(rest) = t.strip_prefix("class=") { ("class_eq", rest) }
        else if let Some(rest) = t.strip_prefix("title=") { ("title_eq", rest) }
        else if let Some(rest) = t.strip_prefix("title_contains=") { ("title_contains", rest) }
        else { ("default", t) };

    let p = pat.to_ascii_lowercase();
    match mode {
        "class_eq" => wm_class.map(|s| s.eq_ignore_ascii_case(pat)).unwrap_or(false),
        "title_eq" => title.map(|s| s.eq_ignore_ascii_case(pat)).unwrap_or(false),
        "title_contains" => title.map(|s| s.to_ascii_lowercase().contains(&p)).unwrap_or(false),
        _ => {
            // Default: if WM_CLASS is present, require exact match (case-insensitive).
            // Fallback to title contains only when class is unavailable.
            if let Some(cls) = wm_class { if cls.eq_ignore_ascii_case(pat) { return true; } }
            if wm_class.is_none() { if let Some(ti) = title { return ti.to_ascii_lowercase().contains(&p); } }
            false
        }
    }
}

pub fn have_atoms(supported: &[u64], required: &[u64]) -> bool {
    required.iter().all(|r| supported.iter().any(|s| s == r))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub window: u64,
    pub on_current_ws: bool,
    pub visible: bool,
}

// Selection policy: prefer on-current-workspace and visible; then on-current-workspace hidden;
// then any visible; finally any.
pub fn select_preferred_window(candidates: &[Candidate]) -> Option<u64> {
    if candidates.is_empty() { return None; }
    if let Some(c) = candidates.iter().find(|c| c.on_current_ws && c.visible) { return Some(c.window); }
    if let Some(c) = candidates.iter().find(|c| c.on_current_ws && !c.visible) { return Some(c.window); }
    if let Some(c) = candidates.iter().find(|c| !c.on_current_ws && c.visible) { return Some(c.window); }
    Some(candidates[0].window)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_desktop_message_shape() {
        let spec = build_net_wm_desktop_message(0x1234, 3, 0x55Au64);
        assert_eq!(spec.message_type_atom, 0x55Au64);
        assert_eq!(spec.window, 0x1234);
        assert_eq!(spec.data[0], 3);
        assert_eq!(spec.data[1], 1); // application
    }

    #[test]
    fn build_activate_message_shape() {
        let spec = build_net_active_window_message(0x9999, 0x77Bu64);
        assert_eq!(spec.message_type_atom, 0x77Bu64);
        assert_eq!(spec.window, 0x9999);
        assert_eq!(spec.data[0], 1);
        assert_eq!(spec.data[1], 0);
    }

    #[test]
    fn app_match_by_title_or_class() {
        assert!(matches_app("Alacritty", Some("Terminal — Alacritty"), None));
        assert!(matches_app("alacritty", None, Some("Alacritty")));
        assert!(!matches_app("Alacritty", Some("Other"), Some("OtherApp")));
    }

    #[test]
    fn app_match_explicit_modes() {
        // class equals
        assert!(matches_app("class=Alacritty", None, Some("alacritty")));
        assert!(!matches_app("class=Alacritty", None, Some("org.alacritty")));
        // title equals
        assert!(matches_app("title=MyTerm", Some("myterm"), None));
        assert!(!matches_app("title=MyTerm", Some("Other MyTerm!"), None));
        // title contains
        assert!(matches_app("title_contains=MyTerm", Some("Other MyTerm!"), None));
    }

    #[test]
    fn check_have_atoms() {
        let supported = [1u64, 10, 100, 1_000, 42];
        assert!(have_atoms(&supported, &[10, 42]));
        assert!(!have_atoms(&supported, &[999]));
    }

    #[test]
    fn selection_prefers_current_visible_then_current_hidden() {
        let cands = vec![
            Candidate { window: 10, on_current_ws: false, visible: true },
            Candidate { window: 11, on_current_ws: true,  visible: false },
            Candidate { window: 12, on_current_ws: true,  visible: true },
        ];
        // Should pick 12 (current & visible)
        assert_eq!(select_preferred_window(&cands), Some(12));

        let cands2 = vec![
            Candidate { window: 20, on_current_ws: false, visible: true },
            Candidate { window: 21, on_current_ws: true,  visible: false },
        ];
        // No current-visible: pick 21 (current & hidden)
        assert_eq!(select_preferred_window(&cands2), Some(21));
    }

    #[test]
    fn selection_falls_back_to_visible_then_any() {
        let cands = vec![
            Candidate { window: 30, on_current_ws: false, visible: true },
            Candidate { window: 31, on_current_ws: false, visible: false },
        ];
        assert_eq!(select_preferred_window(&cands), Some(30));

        let cands2 = vec![
            Candidate { window: 40, on_current_ws: false, visible: false },
            Candidate { window: 41, on_current_ws: false, visible: false },
        ];
        assert_eq!(select_preferred_window(&cands2), Some(40));
    }
}
