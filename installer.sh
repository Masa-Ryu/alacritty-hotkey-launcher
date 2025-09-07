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

# Detect current DISPLAY
CURRENT_DISPLAY="${DISPLAY:-:0}"

mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/alacritty-hotkey-launcher.service <<EOF
[Unit]
Description=Alacritty Hotkey Launcher
After=graphical-session.target

[Service]
ExecStart=%h/.local/bin/alacritty-hotkey-launcher
Environment=ALACRITTY_HOTKEY_LAUNCHER_CONFIG=%h/.config/alacritty-hotkey-launcher/config.toml
Environment=DISPLAY=$CURRENT_DISPLAY
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now alacritty-hotkey-launcher

echo "Installation completed!"
echo ""
echo "Service status:"
systemctl --user status alacritty-hotkey-launcher --no-pager -l
echo ""
echo "If the service fails, check logs with:"
echo "  journalctl --user -u alacritty-hotkey-launcher -f"
echo ""
echo "Manual restart:"
echo "  systemctl --user restart alacritty-hotkey-launcher"
