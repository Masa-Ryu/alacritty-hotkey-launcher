#!/bin/bash

cargo build --release
if [ $? -ne 0 ]; then
    echo "Error: cargo build failed. Aborting installation."
    exit 1
fi

mkdir -p ~/.local/bin
cp target/release/alacritty-hotkey-launcher ~/.local/bin/alacritty-hotkey-launcher

mkdir -p ~/.config/alacritty-hotkey-launcher

# Detect Alacritty path
ALACRITTY_PATH="$(which alacritty 2>/dev/null)"
if [ -z "$ALACRITTY_PATH" ]; then
    echo "Warning: Could not detect Alacritty binary with 'which alacritty'. Using default path '/usr/bin/alacritty'."
    ALACRITTY_PATH="/usr/bin/alacritty"
fi

cat > ~/.config/alacritty-hotkey-launcher/config.toml <<EOF
[settings]
interval = 300
app_path = "$ALACRITTY_PATH"
app_name = "class=Alacritty"
detected_key = "ctrl_left"
EOF

mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/alacritty-hotkey-launcher.service <<'EOF'
[Unit]
Description=Alacritty Hotkey Launcher
After=graphical.target

[Service]
Type=simple
ExecStart=%h/.local/bin/alacritty-hotkey-launcher
Environment=ALACRITTY_HOTKEY_LAUNCHER_CONFIG=%h/.config/alacritty-hotkey-launcher/config.toml
Restart=on-failure
RestartSec=5
KillMode=mixed
TimeoutStopSec=5

[Install]
WantedBy=default.target
EOF

# Create a wrapper script that sets up environment properly
cat > ~/.local/bin/alacritty-hotkey-launcher-wrapper <<'EOF'
#!/bin/bash
# Import user session environment variables
if [ -z "$DISPLAY" ] && [ -z "$WAYLAND_DISPLAY" ]; then
    # Try to detect display environment
    if [ -n "$XDG_SESSION_TYPE" ]; then
        case "$XDG_SESSION_TYPE" in
            "x11")
                export DISPLAY="${DISPLAY:-:0}"
                ;;
            "wayland")
                export WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-0}"
                ;;
        esac
    fi
fi

# Execute the actual launcher
exec ~/.local/bin/alacritty-hotkey-launcher "$@"
EOF

chmod +x ~/.local/bin/alacritty-hotkey-launcher-wrapper

# Update service to use wrapper
sed -i 's|ExecStart=%h/.local/bin/alacritty-hotkey-launcher|ExecStart=%h/.local/bin/alacritty-hotkey-launcher-wrapper|' ~/.config/systemd/user/alacritty-hotkey-launcher.service

systemctl --user daemon-reload
systemctl --user enable alacritty-hotkey-launcher
systemctl --user start alacritty-hotkey-launcher

echo "Installation completed!"
echo ""
echo "Service status:"
systemctl --user status alacritty-hotkey-launcher --no-pager -l
echo ""
echo "Environment check:"
echo "  DISPLAY: ${DISPLAY:-not set}"
echo "  WAYLAND_DISPLAY: ${WAYLAND_DISPLAY:-not set}"
echo "  XDG_SESSION_TYPE: ${XDG_SESSION_TYPE:-not set}"
echo "  Desktop session: ${XDG_CURRENT_DESKTOP:-not set}"
echo ""
echo "If the service fails, check logs with:"
echo "  journalctl --user -u alacritty-hotkey-launcher -f"
echo ""
echo "Troubleshooting commands:"
echo "  systemctl --user restart alacritty-hotkey-launcher"
echo "  systemctl --user status alacritty-hotkey-launcher"
echo "  ~/.local/bin/alacritty-hotkey-launcher-wrapper  # test manually"
