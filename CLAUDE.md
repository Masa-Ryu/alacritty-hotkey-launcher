# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

### Building and Running
```bash
# Build in debug mode
cargo build

# Build release version (required for deployment)
cargo build --release

# Run the application directly
./target/release/alacritty-hotkey-launcher

# Run with custom config
ALACRITTY_HOTKEY_LAUNCHER_CONFIG=/path/to/config.toml cargo run
```

### Testing and Code Quality
```bash
# Run all tests (includes unit tests for core logic)
cargo test

# Run tests with verbose output
cargo test --verbose

# Run a single test by name
cargo test double_press_detection

# Lint code with clippy
cargo clippy --all-targets --all-features

# Check code formatting
cargo fmt --all --check

# Auto-format code
cargo fmt --all
```

### System Dependencies
Required for building on Ubuntu/Debian:
```bash
sudo apt update
sudo apt install -y build-essential pkg-config libx11-dev libxi-dev libxtst-dev
```

## Architecture Overview

This is a Rust application that provides hotkey-based window management for Alacritty terminal. The codebase uses a modular backend architecture to support different display systems (X11/Wayland).

### Core Components

- **`src/main.rs`**: Entry point with event loop and backend selection logic. Automatically chooses X11 if `DISPLAY` env var exists, otherwise falls back to Wayland.

- **`src/common_backend.rs`**: Core abstraction layer containing:
  - `WindowBackend` trait: Unified interface for window operations across display systems
  - `toggle_or_launch()`: Main orchestration logic for show/hide/workspace-move/launch behavior
  - `DoublePressDetector`: Robust double-tap detection requiring key release between presses
  - `AppConfig`: Configuration structure

- **`src/x11_backend.rs`**: X11 implementation of `WindowBackend` with window search, visibility control, and workspace management

- **`src/wayland_backend.rs`**: Wayland implementation (currently limited to app launching only)

- **`src/x11_ewmh.rs`**: X11 Extended Window Manager Hints utilities

- **`src/config.rs`**: TOML configuration loading with legacy format support

### Key Design Patterns

1. **Backend Abstraction**: The `WindowBackend` trait allows supporting multiple display systems through a common interface
2. **State Machine Logic**: Core toggle behavior is deterministic based on window presence, workspace location, and visibility
3. **TDD Approach**: Core logic has comprehensive unit tests that serve as behavior specification

## Configuration

Configuration file location (in order of precedence):
1. `ALACRITTY_HOTKEY_LAUNCHER_CONFIG` environment variable
2. `src/config.toml` (default)

Key settings in `config.toml`:
- `interval`: Double-tap detection window (milliseconds)
- `app_path`: Alacritty executable path
- `app_name`: Window title substring for identification
- `detected_key`: Trigger key (supports "ctrl_left", "ctrl_right", etc.)

## Window Management Behavior

The application implements this logic:
- **Same workspace + visible**: Hide window
- **Same workspace + hidden**: Show window
- **Different workspace**: Move to current workspace and show
- **Window not found**: Launch new instance

## Testing Strategy

Core business logic is covered by unit tests in `src/common_backend.rs`. Tests use mock backends to verify orchestration behavior and double-press detection timing requirements.