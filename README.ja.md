# Alacritty Hotkey Launcher

左Ctrlのダブルタップで Alacritty を「表示/非表示」切替し、別ワークスペースにある場合は現在のワークスペースへ移動させます。未起動なら起動します。

現在は X11 で実用的に動作します。Wayland は Sway/Hyprland/GNOME に対応し、ウィンドウの検出・移動・表示/非表示をサポートします（Swayはスクラッチパッド、Hyprlandはspecial workspace、GNOMEは最小化/アクティベート）。その他のコンポジタでは起動のみの暫定実装です。

## 特徴
- 左Ctrlのダブルタップでトグル（デフォルト300ms以内、リリース必須で誤検出抑制）
- 同一WSでは表示/非表示を切替、別WSなら移動して表示
- 設定ファイルで間隔/キー/アプリパス/タイトル名を変更可能
- バックエンド分離（X11/Wayland）で拡張しやすい設計

## 動作環境
- Linux X11（Ubuntu 22.04 で動作確認）
- Wayland: Sway/Hyprland/GNOME でトグル動作に対応（他のコンポジタは起動のみ）
- macOS: ビルド時にX11リンクで失敗するため非対応

必要パッケージ例（Ubuntu）:
```
sudo apt update
sudo apt install -y build-essential pkg-config libx11-dev libxi-dev libxtst-dev
```

Alacritty 本体:
- https://github.com/alacritty/alacritty

## ビルドと実行
```
cargo build --release
./target/release/Alacritty-Hotkey-Launcher
```

実行中に左Ctrlを素早く2回押すと、Alacritty がトグルします。

X11/Wayland の自動選択:
- `DISPLAY` があれば X11 バックエンド
- `DISPLAY` が無く `WAYLAND_DISPLAY` がある場合は Wayland バックエンド（Sway/Hyprland/GNOME ならフル機能、その他は起動のみ）

## 設定
設定ファイルの優先順位:
- `ALACRITTY_HOTKEY_LAUNCHER_CONFIG`（絶対パス指定）
- `~/.config/alacritty-hotkey-launcher/config.toml`
- `src/config.toml`（リポジトリ内のデフォルト）

例: `~/.config/alacritty-hotkey-launcher/config.toml`
```toml
[settings]
interval = 300                 # ダブルタップ判定の間隔(ms)
app_path = "/usr/local/bin/alacritty"  # 起動コマンド
app_name = "class=Alacritty"   # 既定: WM_CLASSに厳密一致（推奨）
detected_key = "ctrl_left"     # 検出キー（ctrl_left/ctrl_right など）
wayland_hide_method = "auto"   # Waylandの非表示動作: auto|scratchpad|none
#  - auto: Swayはscratchpad, Hyprlandはspecial workspaceを使用
#  - scratchpad: 常にscratchpad/specialを使用
#  - none: 非表示操作を行わない（表示のみ）
```

互換: 旧表記 `[settigs]` や `detected_keys = ["ctrl_left", ...]` にも対応します（自動補正/先頭キー採用）。

キー表記（大文字小文字無視）:
- `ctrl_left`, `control_left`, `ctrl`, `control`
- `ctrl_right`, `control_right`

アプリ識別の書式（X11/Wayland Sway）:
- `class=Alacritty`: WM_CLASS に厳密一致（推奨、誤検出を防止）
- `title=MyTerm`: タイトルに厳密一致
- `title_contains=Alacritty`: タイトル部分一致（必要な場合のみ）

## 仕様（安定化のための振る舞い）
- ダブルタップ検出は「押下→離す→押下」のみを有効とし、オートリピートを無視
- 同一ワークスペース: 表示中なら隠す、非表示なら表示
- 別ワークスペース: 現在WSへ移動してから表示
- 未起動: `app_path` を起動

備考（X11）:
- ウィンドウ検索はタイトル部分一致（`app_name`）で行っています。複数ウィンドウやタイトル変更で誤検出の可能性があります（今後 `WM_CLASS` 等での堅牢化予定）。
- WS移動は `_NET_WM_DESKTOP` を直接書き換える簡易実装です。EWMHの `ClientMessage` による操作へ置き換える予定です。

備考（Wayland/Sway/Hyprland/GNOME）:
- Sway: `swaymsg -t get_tree`/`get_workspaces` JSON を解析してウィンドウ検出・WS判定を行います。
  - 表示: `[con_id=ID] focus`（スクラッチパッド上にある場合は `scratchpad show` を併用）
  - 非表示: `[con_id=ID] move to scratchpad`（スクラッチパッドへ退避）
  - WS移動: `[con_id=ID] move to workspace current`
- Hyprland: `hyprctl -j clients`/`monitors` JSON を解析してウィンドウ検出・WS判定を行います。
  - 表示: `hyprctl dispatch focuswindow address:0xID`
  - 非表示: `hyprctl dispatch movetoworkspace special`（special workspaceへ退避）
  - WS移動: `hyprctl dispatch movetoworkspace current`
- GNOME: `gdbus` で `org.gnome.Shell` の `Eval` を呼び出し、Shell上のJSからウィンドウ列挙/操作を行います。
  - 表示: `meta_window.activate()`
  - 非表示: `meta_window.minimize()`（設定で `none` にすれば非表示操作を行わない）
  - WS移動: `meta_window.change_workspace(active_ws)`
  - 注意: 一部ディストリでは `Eval` が無効化されている場合があります。その際はフル機能は動作しません（起動のみ）。

## アーキテクチャ
- `src/common_backend.rs`
  - `WindowBackend` トレイト: バックエンド共通のウィンドウ操作IF
  - `toggle_or_launch`: トグルの中核ロジック
  - `DoublePressDetector`: ダブルタップの安定検出
- `src/x11_backend.rs`: X11実装（検索/表示・非表示/WS判定・WS移動/起動）
- `src/wayland_backend.rs`: Wayland実装（Sway/Hyprland/GNOME 対応: 検索/表示・非表示/WS判定・WS移動、他は起動のみ）
- `src/main.rs`: 環境変数からバックエンド選択、イベントループ、設定読込
- `src/config.rs`: TOML設定の読み込み

## テスト（TDD）
中核ロジックはユニットテストで仕様を固定しています。
```
cargo test
```
- オーケストレーション: 表示/非表示・WS移動・未検出時の起動
- ダブルタップ検出: リリース必須/時間閾値
- 設定読込: TOML→`AppConfig`、デフォルト/レガシー表記
- Wayland/Sway: `swaymsg` の JSON をモックしてウィンドウ検出・可視性・コマンド発行を検証
- Wayland/Hyprland: `hyprctl -j` の JSON をモックしてウィンドウ検出・可視性・コマンド発行を検証
- Wayland/GNOME: `gdbus` 経由の Shell Eval をモックしてウィンドウ検出・可視性・コマンド発行を検証

## インストールと自動起動（ユーザー単位）
systemd user サービスでの簡単セットアップ:

```
git clone https://github.com/Masa-Ryu/Alacritty-Hotkey-Launcher.git
cd Alacritty-Hotkey-Launcher
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
```

## 既知の制限・今後の予定
- X11: タイトル一致依存 → `WM_CLASS`/`_NET_CLIENT_LIST`/EWMH ClientMessage 対応へ改善予定
- Wayland: Sway/Hyprland/GNOME 以外のコンポジタ（Wayfire 等）では起動のみ。各アダプタ（DBus/拡張等）対応を拡充予定
- 複数ウィンドウ: どのウィンドウを対象とするかのポリシー追加（最後にフォーカス/最新など）
- 複合ホットキー: ダブルタップ以外の組み合わせ対応

## トラブルシューティング
- 反応しない: X11なら `echo $DISPLAY` を確認。Waylandの場合は `echo $WAYLAND_DISPLAY` とコンポジタの対応状況を確認。
- タイトルやクラスが一致しない: `app_name` を `class=Alacritty` などに調整してください。
- パスが違う: `app_path` を環境に合わせて修正してください。

---
Pull Request/Issue 歓迎です。特に Wayland 各コンポジタ対応と X11/EWMH 準拠化にご協力いただけると嬉しいです。
