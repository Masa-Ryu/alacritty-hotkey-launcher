#!/bin/bash

cargo build --release

mkdir -p ~/.local/bin
cp target/release/Alacritty-Hotkey-Launcher ~/.local/bin/alacritty-hotkey-launcher

mkdir -p ~/.config/alacritty-hotkey-launcher
cat > ~/.config/alacritty-hotkey-launcher/config.toml <<'EOF'
[settings]
interval = 300
app_path = "/usr/bin/alacritty"
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
