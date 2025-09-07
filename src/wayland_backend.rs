use crate::common_backend::{WindowBackend, AppConfig, WaylandHideMethod};
use crate::x11_ewmh::{Candidate, select_preferred_window, matches_app};
use serde::Deserialize;
use std::io;
use std::process::{Command, Stdio};

// Wayland backend with compositor-specific adapters. Supports Sway and Hyprland.
pub struct WaylandBackend {
    runner: Box<dyn Runner>,
    flavor: Option<WlFlavor>,
    hide_method: WaylandHideMethod,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WlFlavor { Sway, Hyprland, Gnome }

impl WaylandBackend {
    pub fn new() -> Self { Self { runner: Box::new(SystemRunner), flavor: None, hide_method: WaylandHideMethod::Auto } }

    pub fn new_with_config(cfg: &AppConfig) -> Self {
        Self { runner: Box::new(SystemRunner), flavor: None, hide_method: cfg.wayland_hide_method }
    }

    // For tests: inject a fake runner
    #[cfg(test)]
    fn with_runner(r: Box<dyn Runner>) -> Self { Self { runner: r, flavor: None, hide_method: WaylandHideMethod::Auto } }
    #[cfg(test)]
    fn with_runner_and_method(r: Box<dyn Runner>, m: WaylandHideMethod) -> Self { Self { runner: r, flavor: None, hide_method: m } }

    fn ensure_flavor(&mut self) -> Option<WlFlavor> {
        if let Some(f) = self.flavor { return Some(f); }
        // Probe Sway
        if let Ok(json) = self.runner.output("swaymsg", &["-t", "get_tree"]) {
            if json.contains("\"type\":\"root\"") || serde_json::from_str::<SwayNode>(&json).is_ok() {
                self.flavor = Some(WlFlavor::Sway);
                return self.flavor;
            }
        }
        // Probe Hyprland
        if let Ok(mon_json) = self.runner.output("hyprctl", &["-j", "monitors"]) {
            if serde_json::from_str::<Vec<HyprMonitor>>(&mon_json).is_ok() {
                self.flavor = Some(WlFlavor::Hyprland);
                return self.flavor;
            }
        }
        // Probe GNOME Shell via DBus Eval
        if let Some(js) = self.gnome_eval_json("JSON.stringify({ok:true})") {
            if js.contains("\"ok\":true") {
                self.flavor = Some(WlFlavor::Gnome);
                return self.flavor;
            }
        }
        None
    }

    // --- Sway helpers ---
    fn sway_current_workspace(&mut self) -> Option<String> {
        let out = self.runner.output("swaymsg", &["-t", "get_workspaces"]).ok()?;
        let workspaces: Vec<SwayWorkspace> = serde_json::from_str(&out).ok()?;
        workspaces.into_iter().find(|w| w.focused).map(|w| w.name)
    }

    fn sway_collect_candidates(&mut self, target: &str) -> io::Result<Vec<Candidate>> {
        let out = self.runner.output("swaymsg", &["-t", "get_tree"]) ?;
        let root: SwayNode = serde_json::from_str(&out)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("parse error: {e}")))?;
        let current_ws = self.sway_current_workspace().unwrap_or_default();
        let mut cands = Vec::new();
        Self::sway_traverse(&root, None, &mut |node, ws_name| {
            if let Some(props) = &node.window_properties {
                let class = props.class.as_deref();
                let title = props.title.as_deref();
                if matches_app(target, title, class) {
                    let ws = ws_name.unwrap_or("");
                    let on_ws = ws == current_ws;
                    let visible = on_ws && ws != "__i3_scratch";
                    cands.push(Candidate { window: node.id, on_current_ws: on_ws, visible });
                }
            }
        });
        Ok(cands)
    }

    fn sway_traverse<'a, F: FnMut(&'a SwayNode, Option<&'a str>)>(node: &'a SwayNode, ws: Option<&'a str>, f: &mut F) {
        let mut this_ws = ws;
        if node.r#type == "workspace" {
            this_ws = node.name.as_deref();
        }
        f(node, this_ws);
        for n in &node.nodes { Self::sway_traverse(n, this_ws, f); }
        for n in &node.floating_nodes { Self::sway_traverse(n, this_ws, f); }
    }

    fn sway_move_to_current_ws(&mut self, id: u64) {
        let _ = self.runner.quiet("swaymsg", &[&format!("[con_id={id}]"), "move", "to", "workspace", "current"]);
    }

    fn sway_focus_or_show(&mut self, id: u64) {
        // Try to focus; if in scratchpad, show from scratchpad then focus
        let _ = self.runner.quiet("swaymsg", &[&format!("[con_id={id}]"), "scratchpad", "show"]);
        let _ = self.runner.quiet("swaymsg", &[&format!("[con_id={id}]"), "focus"]);
    }

    fn sway_hide_to_scratchpad(&mut self, id: u64) {
        let _ = self.runner.quiet("swaymsg", &[&format!("[con_id={id}]"), "move", "to", "scratchpad"]);
    }

    // --- Hyprland helpers ---
    fn hypr_current_workspace_id(&mut self) -> Option<i64> {
        let out = self.runner.output("hyprctl", &["-j", "monitors"]).ok()?;
        let monitors: Vec<HyprMonitor> = serde_json::from_str(&out).ok()?;
        monitors.into_iter().find(|m| m.focused).map(|m| m.activeWorkspace.id)
    }

    fn hypr_collect_candidates(&mut self, target: &str) -> io::Result<Vec<Candidate>> {
        let out = self.runner.output("hyprctl", &["-j", "clients"]) ?;
        let clients: Vec<HyprClient> = serde_json::from_str(&out)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("parse error: {e}")))?;
        let current = self.hypr_current_workspace_id().unwrap_or(-1);
        let mut cands = Vec::new();
        for c in clients {
            let class = Some(c.class.as_str());
            let title = Some(c.title.as_str());
            if matches_app(target, title, class) {
                if let Some(id) = hypr_parse_address(&c.address) {
                    let on_ws = c.workspace.id == current;
                    let visible = on_ws; // special ws not considered here
                    cands.push(Candidate { window: id, on_current_ws: on_ws, visible });
                }
            }
        }
        Ok(cands)
    }

    fn hypr_move_to_current_ws(&mut self, id: u64) {
        // Focus then move focused window to current workspace
        let addr = format!("address:0x{:x}", id);
        let _ = self.runner.quiet("hyprctl", &["dispatch", "focuswindow", &addr]);
        let _ = self.runner.quiet("hyprctl", &["dispatch", "movetoworkspace", "current"]);
    }

    fn hypr_focus_or_show(&mut self, id: u64) {
        let addr = format!("address:0x{:x}", id);
        // Focusing a client should switch to its workspace and reveal it
        let _ = self.runner.quiet("hyprctl", &["dispatch", "focuswindow", &addr]);
    }

    fn hypr_hide_to_special(&mut self, id: u64) {
        // Move target window into special workspace
        let addr = format!("address:0x{:x}", id);
        let _ = self.runner.quiet("hyprctl", &["dispatch", "focuswindow", &addr]);
        let _ = self.runner.quiet("hyprctl", &["dispatch", "movetoworkspace", "special"]);
    }
}

impl WindowBackend for WaylandBackend {
    fn find_window(&mut self, app_name: &str) -> Option<u64> {
        match self.ensure_flavor()? {
            WlFlavor::Sway => {
                let cands = self.sway_collect_candidates(app_name).ok()?;
                select_preferred_window(&cands)
            }
            WlFlavor::Hyprland => {
                let cands = self.hypr_collect_candidates(app_name).ok()?;
                select_preferred_window(&cands)
            }
            WlFlavor::Gnome => {
                let cands = self.gnome_collect_candidates(app_name).ok()?;
                select_preferred_window(&cands)
            }
        }
    }

    fn is_on_current_workspace(&mut self, window: u64) -> bool {
        match self.ensure_flavor() {
            Some(WlFlavor::Sway) => {
                let current = match self.sway_current_workspace() { Some(s) => s, None => return false };
                let out = match self.runner.output("swaymsg", &["-t", "get_tree"]) { Ok(s) => s, Err(_) => return false };
                let root: SwayNode = match serde_json::from_str(&out) { Ok(r) => r, Err(_) => return false };
                let mut on_ws = false;
                Self::sway_traverse(&root, None, &mut |n, ws| {
                    if n.id == window { on_ws = ws == Some(current.as_str()); }
                });
                on_ws
            }
            Some(WlFlavor::Hyprland) => {
                let current = match self.hypr_current_workspace_id() { Some(id) => id, None => return false };
                let out = match self.runner.output("hyprctl", &["-j", "clients"]) { Ok(s) => s, Err(_) => return false };
                let clients: Vec<HyprClient> = match serde_json::from_str(&out) { Ok(v) => v, Err(_) => return false };
                clients.into_iter().any(|c| hypr_parse_address(&c.address) == Some(window) && c.workspace.id == current)
            }
            Some(WlFlavor::Gnome) => {
                let current = match self.gnome_current_workspace() { Some(id) => id, None => return false };
                let wins = match self.gnome_list_windows() { Some(v) => v, None => return false };
                wins.into_iter().any(|w| w.id == window && w.ws == current)
            }
            None => false,
        }
    }

    fn is_visible(&mut self, window: u64) -> bool {
        match self.ensure_flavor() {
            Some(WlFlavor::Sway) => {
                let current = match self.sway_current_workspace() { Some(s) => s, None => return false };
                let out = match self.runner.output("swaymsg", &["-t", "get_tree"]) { Ok(s) => s, Err(_) => return false };
                let root: SwayNode = match serde_json::from_str(&out) { Ok(r) => r, Err(_) => return false };
                let mut visible = false;
                Self::sway_traverse(&root, None, &mut |n, ws| {
                    if n.id == window {
                        if let Some(ws_name) = ws { visible = ws_name == current && ws_name != "__i3_scratch"; }
                    }
                });
                visible
            }
            Some(WlFlavor::Hyprland) => {
                let current = match self.hypr_current_workspace_id() { Some(id) => id, None => return false };
                let out = match self.runner.output("hyprctl", &["-j", "clients"]) { Ok(s) => s, Err(_) => return false };
                let clients: Vec<HyprClient> = match serde_json::from_str(&out) { Ok(v) => v, Err(_) => return false };
                clients.into_iter().any(|c| hypr_parse_address(&c.address) == Some(window) && c.workspace.id == current)
            }
            Some(WlFlavor::Gnome) => {
                let current = match self.gnome_current_workspace() { Some(id) => id, None => return false };
                let wins = match self.gnome_list_windows() { Some(v) => v, None => return false };
                wins.into_iter().any(|w| w.id == window && w.ws == current && !w.minimized)
            }
            None => false,
        }
    }

    fn move_to_current_workspace(&mut self, window: u64) {
        match self.ensure_flavor() {
            Some(WlFlavor::Sway) => { self.sway_move_to_current_ws(window); }
            Some(WlFlavor::Hyprland) => { self.hypr_move_to_current_ws(window); }
            Some(WlFlavor::Gnome) => { self.gnome_move_to_current_ws(window); }
            None => {}
        }
    }

    fn show(&mut self, window: u64) {
        match self.ensure_flavor() {
            Some(WlFlavor::Sway) => { self.sway_focus_or_show(window); }
            Some(WlFlavor::Hyprland) => { self.hypr_focus_or_show(window); }
            Some(WlFlavor::Gnome) => { self.gnome_focus_or_show(window); }
            None => {}
        }
    }

    fn hide(&mut self, window: u64) {
        match (self.ensure_flavor(), self.hide_method) {
            (Some(WlFlavor::Sway), WaylandHideMethod::Scratchpad | WaylandHideMethod::Auto) => self.sway_hide_to_scratchpad(window),
            (Some(WlFlavor::Sway), WaylandHideMethod::None) => { /* no-op */ }
            (Some(WlFlavor::Hyprland), WaylandHideMethod::None) => { /* no-op */ }
            (Some(WlFlavor::Hyprland), _) => self.hypr_hide_to_special(window),
            (Some(WlFlavor::Gnome), WaylandHideMethod::None) => { /* no-op */ }
            (Some(WlFlavor::Gnome), _) => self.gnome_minimize(window),
            _ => {}
        }
    }

    fn launch_app(&mut self, app_path: &str) {
        let _ = Command::new(app_path).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    }
}

// --- Minimal serde models for swaymsg JSON ---
#[derive(Debug, Deserialize)]
struct SwayWorkspace {
    name: String,
    #[serde(default)]
    focused: bool,
}

#[derive(Debug, Deserialize)]
struct SwayWindowProps {
    #[serde(default)]
    class: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SwayNode {
    id: u64,
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "type")]
    r#type: String,
    #[serde(default)]
    nodes: Vec<SwayNode>,
    #[serde(default)]
    floating_nodes: Vec<SwayNode>,
    #[serde(default)]
    window_properties: Option<SwayWindowProps>,
}

// --- Hyprland JSON models ---
#[derive(Debug, Deserialize)]
struct HyprWorkspaceRef { id: i64, name: String }

#[derive(Debug, Deserialize)]
struct HyprClient { address: String, class: String, title: String, workspace: HyprWorkspaceRef }

#[derive(Debug, Deserialize)]
struct HyprActiveWorkspace { id: i64, name: String }

#[derive(Debug, Deserialize)]
struct HyprMonitor { #[serde(default)] focused: bool, #[allow(dead_code)] id: i64, activeWorkspace: HyprActiveWorkspace }

fn hypr_parse_address(s: &str) -> Option<u64> {
    // formats like "0x1a2b3c"; be tolerant with or without 0x
    let t = s.trim();
    let hex = t.strip_prefix("0x").unwrap_or(t);
    u64::from_str_radix(hex, 16).ok()
}

// --- GNOME helpers and models ---
#[derive(Debug, Deserialize, Clone)]
struct GnomeWin { id: u64, #[serde(default)] class: Option<String>, #[serde(default)] title: Option<String>, ws: i64, #[serde(default)] minimized: bool }

impl WaylandBackend {
    fn gnome_eval_json(&mut self, js: &str) -> Option<String> {
        // gdbus returns: (true, '...json...')
        let out = self.runner.output(
            "gdbus",
            &[
                "call", "--session",
                "--dest", "org.gnome.Shell",
                "--object-path", "/org/gnome/Shell",
                "--method", "org.gnome.Shell.Eval",
                js,
            ],
        ).ok()?;
        parse_gdbus_eval_output(&out)
    }

    fn gnome_list_windows(&mut self) -> Option<Vec<GnomeWin>> {
        let js = "JSON.stringify(global.get_window_actors().map(w=>{const m=w.meta_window;return {id:m.get_stable_sequence(),title:m.get_title(),class:m.get_wm_class(),ws:(m.get_workspace()?m.get_workspace().index():-1),minimized:m.minimized};}))";
        let json = self.gnome_eval_json(js)?;
        serde_json::from_str::<Vec<GnomeWin>>(&json).ok()
    }

    fn gnome_current_workspace(&mut self) -> Option<i64> {
        let js = "JSON.stringify({ws: global.workspace_manager.get_active_workspace().index()})";
        let json = self.gnome_eval_json(js)?;
        let v: serde_json::Value = serde_json::from_str(&json).ok()?;
        v.get("ws")?.as_i64()
    }

    fn gnome_minimize(&mut self, id: u64) {
        let js = format!("(() => {{ let m=global.get_window_actors().map(w=>w.meta_window).find(m=>m.get_stable_sequence()=={}); if(m) m.minimize(); return true; }})()", id);
        let _ = self.runner.quiet(
            "gdbus",
            &["call","--session","--dest","org.gnome.Shell","--object-path","/org/gnome/Shell","--method","org.gnome.Shell.Eval", &js]
        );
    }

    fn gnome_focus_or_show(&mut self, id: u64) {
        let js = format!("(() => {{ let m=global.get_window_actors().map(w=>w.meta_window).find(m=>m.get_stable_sequence()=={}); if(m) m.activate(global.get_current_time()); return true; }})()", id);
        let _ = self.runner.quiet(
            "gdbus",
            &["call","--session","--dest","org.gnome.Shell","--object-path","/org/gnome/Shell","--method","org.gnome.Shell.Eval", &js]
        );
    }

    fn gnome_move_to_current_ws(&mut self, id: u64) {
        let js = format!("(() => {{ let m=global.get_window_actors().map(w=>w.meta_window).find(m=>m.get_stable_sequence()=={}); if(m) m.change_workspace(global.workspace_manager.get_active_workspace()); return true; }})()", id);
        let _ = self.runner.quiet(
            "gdbus",
            &["call","--session","--dest","org.gnome.Shell","--object-path","/org/gnome/Shell","--method","org.gnome.Shell.Eval", &js]
        );
    }

    fn gnome_collect_candidates(&mut self, target: &str) -> io::Result<Vec<Candidate>> {
        let wins = self.gnome_list_windows().ok_or_else(|| io::Error::new(io::ErrorKind::Other, "gnome list windows failed"))?;
        let current = self.gnome_current_workspace().unwrap_or(-1);
        let mut cands = Vec::new();
        for w in wins {
            let class = w.class.as_deref();
            let title = w.title.as_deref();
            if matches_app(target, title, class) {
                let on_ws = w.ws == current;
                let visible = on_ws && !w.minimized;
                cands.push(Candidate { window: w.id, on_current_ws: on_ws, visible });
            }
        }
        Ok(cands)
    }
}

fn parse_gdbus_eval_output(s: &str) -> Option<String> {
    // Expect format: (true, '...') or (false, '...')
    let trimmed = s.trim();
    if !trimmed.starts_with('(') { return None; }
    if !trimmed.contains("true") { return None; }
    let first = trimmed.find('\'')?;
    let last = trimmed.rfind('\'')?;
    if last <= first { return None; }
    Some(trimmed[first+1..last].to_string())
}

// --- Command runner abstraction for testability ---
trait Runner {
    fn output(&mut self, program: &str, args: &[&str]) -> io::Result<String>;
    fn quiet(&mut self, program: &str, args: &[&str]) -> io::Result<()>;
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

struct SystemRunner;
impl Runner for SystemRunner {
    fn output(&mut self, program: &str, args: &[&str]) -> io::Result<String> {
        let out = Command::new(program).args(args).output()?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).to_string())
        } else {
            Err(io::Error::new(io::ErrorKind::Other, String::from_utf8_lossy(&out.stderr).to_string()))
        }
    }
    fn quiet(&mut self, program: &str, args: &[&str]) -> io::Result<()> {
        let status = Command::new(program).args(args).stdout(Stdio::null()).stderr(Stdio::null()).status()?;
        if status.success() { Ok(()) } else { Err(io::Error::new(io::ErrorKind::Other, format!("{program} failed"))) }
    }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeRunner { tree_json: String, ws_json: String, hypr_clients: String, hypr_monitors: String, gnome_windows: String, gnome_ws: String, invoked: Vec<(String, Vec<String>)> }

    impl FakeRunner {
        fn new(tree_json: &str, ws_json: &str) -> Self { Self { tree_json: tree_json.to_string(), ws_json: ws_json.to_string(), hypr_clients: String::new(), hypr_monitors: String::new(), gnome_windows: String::new(), gnome_ws: String::new(), invoked: Vec::new() } }
        fn with_hypr(clients: &str, monitors: &str) -> Self { Self { tree_json: String::new(), ws_json: String::new(), hypr_clients: clients.to_string(), hypr_monitors: monitors.to_string(), gnome_windows: String::new(), gnome_ws: String::new(), invoked: Vec::new() } }
        fn with_gnome(windows: &str, ws: &str) -> Self { Self { tree_json: String::new(), ws_json: String::new(), hypr_clients: String::new(), hypr_monitors: String::new(), gnome_windows: windows.to_string(), gnome_ws: ws.to_string(), invoked: Vec::new() } }
    }

    impl Runner for FakeRunner {
        fn output(&mut self, program: &str, args: &[&str]) -> io::Result<String> {
            let a = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
            self.invoked.push((program.to_string(), a.clone()));
            if program == "swaymsg" && args == ["-t", "get_tree"] {
                Ok(self.tree_json.clone())
            } else if program == "swaymsg" && args == ["-t", "get_workspaces"] {
                Ok(self.ws_json.clone())
            } else if program == "hyprctl" && args == ["-j", "clients"] {
                Ok(self.hypr_clients.clone())
            } else if program == "hyprctl" && args == ["-j", "monitors"] {
                Ok(self.hypr_monitors.clone())
            } else if program == "gdbus" && args.len() >= 9 && args[0] == "call" && args[3] == "org.gnome.Shell" && args[5] == "/org/gnome/Shell" && args[7] == "org.gnome.Shell.Eval" {
                // Return GNOME eval results
                let js = args[8].to_string();
                self.invoked.push((program.to_string(), args.iter().map(|s| s.to_string()).collect()));
                if js.contains("get_window_actors") {
                    Ok(format!("(true, '{}')", self.gnome_windows.replace("'", "\\'")))
                } else if js.contains("workspace_manager.get_active_workspace") {
                    Ok(format!("(true, '{}')", self.gnome_ws.replace("'", "\\'")))
                } else {
                    Ok("(true, '{\"ok\":true}')".to_string())
                }
            } else {
                Err(io::Error::new(io::ErrorKind::Other, "unknown command"))
            }
        }
        fn quiet(&mut self, program: &str, args: &[&str]) -> io::Result<()> {
            self.invoked.push((program.to_string(), args.iter().map(|s| s.to_string()).collect()));
            Ok(())
        }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    }

    fn sample_tree() -> String {
        // Minimal but structurally valid sway tree: current ws has Alacritty id 101; scratchpad has id 103
        r#"{
          "id": 1, "name": "root", "type": "root", "nodes": [
            {"id": 10, "name": "WL-1", "type": "output", "nodes": [
              {"id": 20, "name": "1: term", "type": "workspace", "nodes": [
                {"id": 101, "type":"con", "nodes":[], "floating_nodes":[],
                  "window_properties": {"class":"Alacritty", "title":"Alacritty"}}
              ], "floating_nodes": []},
              {"id": 21, "name": "2: web", "type": "workspace", "nodes": [
                {"id": 102, "type":"con", "nodes":[], "floating_nodes":[],
                  "window_properties": {"class":"Firefox", "title":"Mozilla Firefox"}}
              ], "floating_nodes": []},
              {"id": 22, "name": "__i3_scratch", "type": "workspace", "nodes": [
                {"id": 103, "type":"con", "nodes":[], "floating_nodes":[],
                  "window_properties": {"class":"Alacritty", "title":"Scratch Term"}}
              ], "floating_nodes": []}
            ]}
          ],
          "floating_nodes": []
        }"#.to_string()
    }

    fn sample_workspaces_current1() -> String {
        r#"[
          {"name":"1: term", "focused": true},
          {"name":"2: web", "focused": false},
          {"name":"__i3_scratch", "focused": false}
        ]"#.to_string()
    }

    fn hypr_clients_json() -> String {
        r#"[
          {"address":"0x101", "class":"Alacritty", "title":"Alacritty", "workspace": {"id": 1, "name": "1"}},
          {"address":"0x102", "class":"Firefox", "title":"Mozilla Firefox", "workspace": {"id": 2, "name": "2"}}
        ]"#.to_string()
    }

    fn hypr_monitors_json() -> String {
        r#"[
          {"id":0, "focused": true, "activeWorkspace": {"id":1, "name":"1"}},
          {"id":1, "focused": false, "activeWorkspace": {"id":2, "name":"2"}}
        ]"#.to_string()
    }

    fn gnome_windows_json() -> String {
        r#"[
          {"id":1001, "class":"Alacritty", "title":"Alacritty", "ws":1, "minimized": false},
          {"id":1002, "class":"Firefox", "title":"Mozilla Firefox", "ws":2, "minimized": false}
        ]"#.to_string()
    }

    fn gnome_ws_json() -> String { r#"{"ws":1}"#.to_string() }

    #[test]
    fn sway_find_prefers_current_ws_visible() {
        let runner = Box::new(FakeRunner::new(&sample_tree(), &sample_workspaces_current1()));
        let mut be = WaylandBackend::with_runner(runner);
        // class match
        let id = be.find_window("class=Alacritty");
        assert_eq!(id, Some(101));
        // default match should also hit class when available
        let id2 = be.find_window("Alacritty");
        assert_eq!(id2, Some(101));
    }

    #[test]
    fn sway_visibility_and_ws_detection() {
        let runner = Box::new(FakeRunner::new(&sample_tree(), &sample_workspaces_current1()));
        let mut be = WaylandBackend::with_runner(runner);
        assert!(be.is_on_current_workspace(101));
        assert!(be.is_visible(101));
        // scratchpad is not on current WS and not visible
        assert!(!be.is_on_current_workspace(103));
        assert!(!be.is_visible(103));
    }

    #[test]
    fn sway_commands_for_move_show_hide() {
        let runner = Box::new(FakeRunner::new(&sample_tree(), &sample_workspaces_current1()));
        let mut be = WaylandBackend::with_runner(runner);
        be.move_to_current_workspace(101);
        be.show(101);
        be.hide(101);
        // Verify commands recorded order and content
        let fr = be.runner.as_any().downcast_ref::<FakeRunner>().expect("expected FakeRunner");
        let cmds: Vec<String> = fr.invoked.iter().map(|(p,a)| format!("{} {}", p, a.join(" "))).collect();
        assert!(cmds.iter().any(|c| c.contains("swaymsg [con_id=101] move to workspace current")));
        assert!(cmds.iter().any(|c| c.contains("swaymsg [con_id=101] scratchpad show")));
        assert!(cmds.iter().any(|c| c.contains("swaymsg [con_id=101] focus")));
        assert!(cmds.iter().any(|c| c.contains("swaymsg [con_id=101] move to scratchpad")));
    }

    #[test]
    fn sway_hide_none_does_not_use_scratchpad() {
        let runner = Box::new(FakeRunner::new(&sample_tree(), &sample_workspaces_current1()));
        let mut be = WaylandBackend::with_runner_and_method(runner, WaylandHideMethod::None);
        be.hide(101);
        let fr = be.runner.as_any().downcast_ref::<FakeRunner>().expect("expected FakeRunner");
        let cmds: Vec<String> = fr.invoked.iter().map(|(p,a)| format!("{} {}", p, a.join(" "))).collect();
        assert!(!cmds.iter().any(|c| c.contains("move to scratchpad")));
    }

    #[test]
    fn hypr_find_and_visibility() {
        let runner = Box::new(FakeRunner::with_hypr(&hypr_clients_json(), &hypr_monitors_json()));
        let mut be = WaylandBackend::with_runner(runner);
        let id = be.find_window("class=Alacritty");
        assert_eq!(id, Some(0x101));
        assert!(be.is_on_current_workspace(0x101));
        assert!(be.is_visible(0x101));
        assert!(!be.is_on_current_workspace(0x102));
    }

    #[test]
    fn hypr_commands_for_move_show_hide() {
        let runner = Box::new(FakeRunner::with_hypr(&hypr_clients_json(), &hypr_monitors_json()));
        let mut be = WaylandBackend::with_runner(runner);
        be.move_to_current_workspace(0x101);
        be.show(0x101);
        be.hide(0x101);
        let fr = be.runner.as_any().downcast_ref::<FakeRunner>().expect("expected FakeRunner");
        let cmds: Vec<String> = fr.invoked.iter().map(|(p,a)| format!("{} {}", p, a.join(" "))).collect();
        assert!(cmds.iter().any(|c| c.contains("hyprctl dispatch focuswindow address:0x101")));
        assert!(cmds.iter().any(|c| c.contains("hyprctl dispatch movetoworkspace current")));
        assert!(cmds.iter().any(|c| c.contains("hyprctl dispatch movetoworkspace special")));
    }

    #[test]
    fn gnome_find_visibility_and_commands() {
        let runner = Box::new(FakeRunner::with_gnome(&gnome_windows_json(), &gnome_ws_json()));
        let mut be = WaylandBackend::with_runner(runner);
        // Find
        let id = be.find_window("class=Alacritty");
        assert_eq!(id, Some(1001));
        // Visibility
        assert!(be.is_on_current_workspace(1001));
        assert!(be.is_visible(1001));
        // Commands
        be.move_to_current_workspace(1001);
        be.show(1001);
        be.hide(1001);
        let fr = be.runner.as_any().downcast_ref::<FakeRunner>().expect("expected FakeRunner");
        let cmds: Vec<String> = fr.invoked.iter().map(|(p,a)| format!("{} {}", p, a.join(" "))).collect();
        assert!(cmds.iter().any(|c| c.contains("gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell --method org.gnome.Shell.Eval")));
        assert!(cmds.iter().any(|c| c.contains("minimize")));
        assert!(cmds.iter().any(|c| c.contains("activate")));
        assert!(cmds.iter().any(|c| c.contains("change_workspace")));
    }
}

// no extra helpers needed now that Runner exposes as_any
