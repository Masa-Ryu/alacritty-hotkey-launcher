# Alacritty Hotkey Launcher

Double‑tap Left Ctrl to toggle Alacritty: show/hide when on the current workspace, move it to the current workspace when it’s elsewhere, and launch it if it’s not running.

X11 is fully supported.

## Features
- Double‑tap Left Ctrl toggle (300 ms by default, requires release to avoid repeats)
- Show/hide on the same workspace; move to current workspace otherwise
- Configurable interval/key/app path/app identifier
- Pluggable backends (X11) with unit‑tested core logic

## Requirements
- Linux X11 (verified on Ubuntu 22.04)
- macOS: not supported (X11 link fails at build time)

Ubuntu packages for X11 builds:
```
sudo apt update
sudo apt install -y build-essential pkg-config libx11-dev libxi-dev libxtst-dev
```

Alacritty itself:
- https://github.com/alacritty/alacritty

## Build & Run
```
git clone https://github.com/Masa-Ryu/Alacritty-Hotkey-Launcher.git
cd Alacritty-Hotkey-Launcher
./installer.sh
```

While running, double‑tap Left Ctrl to toggle Alacritty.

Backend auto‑selection:
- If `DISPLAY` is set → X11 backend
- If `DISPLAY` is unset and `WAYLAND_DISPLAY` is set → Wayland backend (full features on Sway/Hyprland/GNOME; launch‑only otherwise)

## Configuration
Config precedence:
- `ALACRITTY_HOTKEY_LAUNCHER_CONFIG` (absolute path)
- `~/.config/alacritty-hotkey-launcher/config.toml`
- `src/config.toml` (repo default)

Example: `~/.config/alacritty-hotkey-launcher/config.toml`
```
[settings]
interval = 300                 # double‑tap interval (ms)
app_path = "/usr/local/bin/alacritty"  # launch command
app_name = "class=Alacritty"   # exact WM_CLASS match (recommended)
detected_key = "ctrl_left"     # detection key (e.g. ctrl_left/ctrl_right)
wayland_hide_method = "auto"   # Wayland hide behavior: auto|scratchpad|none
#  - auto: Sway uses scratchpad; Hyprland uses special workspace
#  - scratchpad: always use scratchpad/special to hide
#  - none: do not hide (only show)
```

Compatibility notes:
- Legacy `[settigs]` header and `detected_keys = ["ctrl_left", ...]` are accepted (auto‑fixed/first element used).

Key names (case‑insensitive):
- `ctrl_left`, `control_left`, `ctrl`, `control`
- `ctrl_right`, `control_right`

App identifier formats (X11 and Wayland):
- `class=Alacritty`: exact WM_CLASS match (recommended)
- `title=MyTerm`: exact title match
- `title_contains=Alacritty`: partial title match (use cautiously)

## Behavior
- Double tap requires “press → release → press” and ignores key auto‑repeat
- Same workspace: hide if visible, show if hidden
- Different workspace: move to current workspace then show
- Not running: launch `app_path`

Notes (X11):
- Window discovery prefers WM_CLASS exact matches via `app_name`.
- Workspace move uses EWMH (`_NET_WM_DESKTOP`) via ClientMessage when available.


## Architecture
- `src/common_backend.rs`: window backend trait, toggle orchestrator, double‑press detector
- `src/x11_backend.rs`: X11 backend (find/show/hide/workspace/move/launch)
- `src/wayland_backend.rs`: Wayland backend (Sway/Hyprland/GNOME full; others launch‑only)
- `src/main.rs`: backend selection, event loop, config loading
- `src/config.rs`: TOML config parsing

## Known limitations / Roadmap
- X11: improve robustness around multi‑window selection policies
- Wayland: adapters for other compositors (e.g., KDE KWin, Wayfire) are planned
- Multi‑window: policy options (last focused, most recent) to be added
- Hotkeys: support combinations beyond “double tap”

## Troubleshooting
- Not responding: on X11 check `echo $DISPLAY`. On Wayland check `echo $WAYLAND_DISPLAY` and the compositor.
- Matching fails: adjust `app_name` (e.g., use `class=Alacritty`).
- Wrong path: update `app_path` for your environment.
