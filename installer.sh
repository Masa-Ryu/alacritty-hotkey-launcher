#!/bin/bash

cargo build --release

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

[Service]
ExecStart=%h/.local/bin/alacritty-hotkey-launcher
Environment=ALACRITTY_HOTKEY_LAUNCHER_CONFIG=%h/.config/alacritty-hotkey-launcher/config.toml
Restart=on-failure

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now alacritty-hotkey-launcher
