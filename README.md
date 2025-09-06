# Alacritty Hotkey Launcher

左Ctrlのダブルタップで Alacritty を「表示/非表示」切替し、別ワークスペースにある場合は現在のワークスペースへ移動させます。未起動なら起動します。

現在は X11 で実用的に動作します。Wayland はコンポジタごとの制約があるため暫定実装（起動のみ）です。

## 特徴
- 左Ctrlのダブルタップでトグル（デフォルト300ms以内、リリース必須で誤検出抑制）
- 同一WSでは表示/非表示を切替、別WSなら移動して表示
- 設定ファイルで間隔/キー/アプリパス/タイトル名を変更可能
- バックエンド分離（X11/Wayland）で拡張しやすい設計

## 動作環境
- Linux X11（Ubuntu 22.04 で動作確認）
- Wayland: 現状は起動のみ（Sway/Hyprland/GNOME向け拡張は今後対応予定）
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
- `DISPLAY` が無く `WAYLAND_DISPLAY` がある場合は Wayland バックエンド（現状は起動のみ）

## 設定
設定ファイルはデフォルトで `src/config.toml` を読み込みます。環境変数 `ALACRITTY_HOTKEY_LAUNCHER_CONFIG` でパスを上書きできます。

例: `src/config.toml`
```toml
[settings]
interval = 300                 # ダブルタップ判定の間隔(ms)
app_path = "/usr/local/bin/alacritty"  # 起動コマンド
app_name = "Alacritty"         # ウィンドウタイトルの一部（X11）
detected_key = "ctrl_left"     # 検出キー（ctrl_left/ctrl_right など）
```

互換: 旧表記 `[settigs]` や `detected_keys = ["ctrl_left", ...]` にも対応します（自動補正/先頭キー採用）。

サポートする主なキー表記（大文字小文字無視）:
- `ctrl_left`, `control_left`, `ctrl`, `control`
- `ctrl_right`, `control_right`

## 仕様（安定化のための振る舞い）
- ダブルタップ検出は「押下→離す→押下」のみを有効とし、オートリピートを無視
- 同一ワークスペース: 表示中なら隠す、非表示なら表示
- 別ワークスペース: 現在WSへ移動してから表示
- 未起動: `app_path` を起動

備考（X11）:
- ウィンドウ検索はタイトル部分一致（`app_name`）で行っています。複数ウィンドウやタイトル変更で誤検出の可能性があります（今後 `WM_CLASS` 等での堅牢化予定）。
- WS移動は `_NET_WM_DESKTOP` を直接書き換える簡易実装です。EWMHの `ClientMessage` による操作へ置き換える予定です。

## アーキテクチャ
- `src/common_backend.rs`
  - `WindowBackend` トレイト: バックエンド共通のウィンドウ操作IF
  - `toggle_or_launch`: トグルの中核ロジック
  - `DoublePressDetector`: ダブルタップの安定検出
- `src/x11_backend.rs`: X11実装（検索/表示・非表示/WS判定・WS移動/起動）
- `src/wayland_backend.rs`: Wayland暫定実装（起動のみ）
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

## 自動起動（例）
systemd user サービス例（X11セッション想定）:
```
~/.config/systemd/user/alacritty-hotkey-launcher.service
```
```ini
[Unit]
Description=Alacritty Hotkey Launcher

[Service]
ExecStart=%h/Development/Alacritty-Hotkey-Launcher/target/release/Alacritty-Hotkey-Launcher
Restart=on-failure
Environment=ALACRITTY_HOTKEY_LAUNCHER_CONFIG=%h/Development/Alacritty-Hotkey-Launcher/src/config.toml

[Install]
WantedBy=default.target
```
有効化:
```
systemctl --user daemon-reload
systemctl --user enable --now alacritty-hotkey-launcher
```

## 既知の制限・今後の予定
- X11: タイトル一致依存 → `WM_CLASS`/`_NET_CLIENT_LIST`/EWMH ClientMessage 対応へ改善予定
- Wayland: グローバル制御はコンポジタ依存（Sway IPC/Hyprland API/GNOME拡張等）→各アダプタを実装予定
- 複数ウィンドウ: どのウィンドウを対象とするかのポリシー追加（最後にフォーカス/最新など）
- 複合ホットキー: ダブルタップ以外の組み合わせ対応

## トラブルシューティング
- 反応しない: X11なら `echo $DISPLAY` を確認。Wayland環境ではX11互換（Xwayland）で動かすか、今後のWayland対応をお待ちください。
- タイトルが一致しない: `app_name` を Alacritty のウィンドウタイトルに合わせてください。
- パスが違う: `app_path` を環境に合わせて修正してください。

---
Pull Request/Issue 歓迎です。特に Wayland 各コンポジタ対応と X11/EWMH 準拠化にご協力いただけると嬉しいです。
