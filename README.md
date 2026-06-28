# J-ALERT Viewer

[SDR# J-Alert デコーダプラグイン](https://github.com/serkenn) が TCP で配信する
JSONL アラートを購読し、**ネイティブGUIの受信表示機**として動作する単体アプリ
（Rust / eframe・egui）です。jars2000 等の自動起動受信機のように、
**待機画面 ↔ 全画面アラート**を切り替えて表示します。

```
 SDR#プラグイン ──TCP 7355(JSONL)──▶ jalert-receiver (ネイティブGUI)
                                          └─(任意) 内蔵Web + cloudflared ──▶ 遠隔から受信箱を閲覧
```

- **単一実行ファイル**。フォント（M PLUS 1p）と Web UI を埋め込み済みで、
  追加ファイル・ランタイム不要。ダブルクリックで GUI 起動。
- ウィンドウGUI（winit + OpenGL）。`F11` でフルスクリーン、`Esc` で解除。

## 画面

アプリ上部のタブで切り替えます。

### 🖥 表示（kiosk）
表示専用端末・電光掲示向け。視認性重視のダーク基調。

| モード | 条件 | 表示 |
|--------|------|------|
| 待機 | 警報・注意報なし | 時計・日付・「受信待機中／異常なし」 |
| 注意報 | 注意報のみ | 待機画面＋下部に黄色の注意報バナー |
| アラート | **警報** | 全画面 **赤**（種別・地域・本文・発表時刻、点滅） |
| アラート | **特別警報** | 全画面 **紫**（同上） |

ポリシーは **警報・特別警報のみ全画面**。注意報はバナー表示にとどめ、対象区域の
全種別が解除されたら待機画面へ復帰します。複数同時発表時は最も重大／最新を全画面に、
残りを下部に一覧表示します。

### 📥 受信箱（管理）
メールボックス風に**全受信を時系列で既読/未読管理**。
[デジタル庁デザインシステム](https://www.digital.go.jp/policies/servicedesign/designsystem)
準拠の配色で、**ライト/ダーク両対応**（上部のテーマ選択：自動／ライト／ダーク）。
行クリックで既読化・本文・発表種別・**XML原文**を確認、「すべて既読」も可能。
再送電文は自動集約して重複表示しません。

## 重大度の判定

各行の全文 JMA XML をパースし、`Body/Warning`（府県予報区）の `<Kind>` の
`Name`＋`Status`（発表/継続/解除）から判定します。

- **特別警報 > 警報 > 注意報**（`Name` の接尾辞で分類）。
- `Status` が `解除`/`なし` の種別は失効として扱う。

## 実行

```sh
# 通常（プラグインの TCP シンクへ接続）
jalert-receiver

# 接続先・ポート指定
jalert-receiver --source-host 127.0.0.1 --source-port 7355

# プラグインなしで動作確認（JSONL を再生）
jalert-receiver --replay path/to/decoded.jsonl --replay-interval 800

# 起動時フルスクリーン
jalert-receiver --fullscreen

# 受信箱を内蔵Webでも公開し、cloudflared で外部からも閲覧
jalert-receiver --cloudflared
```

### オプション / 環境変数

| フラグ | 環境変数 | 既定 | 説明 |
|--------|----------|------|------|
| `--source-host` | `JALERT_SOURCE_HOST` | `127.0.0.1` | プラグインの TCP ホスト |
| `--source-port` | `JALERT_SOURCE_PORT` | `7355` | プラグインの TCP JSONL ポート |
| `--replay FILE` | — | — | JSONL を再生（テスト用） |
| `--replay-interval` | — | `800` | 再生間隔(ms) |
| `--fullscreen` | — | off | 起動時フルスクリーン |
| `--web` | — | off | 受信箱の内蔵 Web サーバを有効化 |
| `--web-port` | `JALERT_WEB_PORT` | `8080` | Web ポート |
| `--cloudflared` | `JALERT_CLOUDFLARED` | off | cloudflared クイックトンネルで外部公開（`--web` も自動有効化） |
| `--cloudflared-bin` | `JALERT_CLOUDFLARED_BIN` | `cloudflared` | cloudflared 実行ファイルのパス |

### Cloudflared 対応
`--cloudflared` を付けると起動時に
[`cloudflared`](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/)
のクイックトンネルを内蔵 Web ポートの前段に立ち上げ、
`https://xxxx.trycloudflare.com` 形式の**公開 URL をコンソールに表示**します。
ファイアウォールのポート開放なしに受信箱をインターネット越しに閲覧できます。

## ビルド

[Rust](https://rustup.rs/)（stable）が必要です。

```sh
cargo build --release          # GUI 版（既定）
cargo test  --no-default-features   # コアの単体テスト（GUI 抜き）
cargo run   -- --replay jalert_test_data/decoded.jsonl --replay-interval 300
```

`--no-default-features` でビルドすると **GUI 抜きのヘッドレス版**（内蔵 Web サーバ
のみ）になります。サーバ常駐用途に。

### Linux のビルド依存
eframe(winit/glow) のため、以下が必要です：

```sh
sudo apt-get install -y libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev \
  libxcb-xfixes0-dev libxkbcommon-dev libssl-dev libgl1-mesa-dev
```

## GPUの無い環境（VM / RDP）での表示

GPU/3Dドライバの無い環境（Proxmox 等の VM、RDP）では OpenGL も Vulkan/DX12 も
利用できず GUI が起動しないことがあります。Windows 版 zip には **Mesa3D の
ソフトウェア OpenGL（`opengl32.dll`）を同梱**しており、exe と同じフォルダに置く
ことでソフトウェア描画で起動します（GPU のある PC では削除すれば GPU 描画に
なります）。起動順序は **glow(OpenGL) → wgpu(DX12/Vulkan/WARP)** で自動選択し、
失敗内容は exe 隣の `jalert-receiver.log` に記録されます。

## 配布 / Release
タグ（`v*`）を push すると GitHub Actions が Linux / Windows のネイティブ実行
ファイルをビルドし、Release に成果物を添付します
（[.github/workflows/release.yml](.github/workflows/release.yml)）。

## ライセンス / クレジット
- 同梱フォント: **M PLUS 1p**（SIL Open Font License 1.1）。
- 本体: MIT（[LICENSE](LICENSE)）。
