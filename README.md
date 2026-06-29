# J-ALERT Viewer

[SDR# J-Alert デコーダプラグイン](https://github.com/serkenn) が TCP で配信する
JSONL 電文を購読し、**ネイティブGUI受信表示機**として動作する
単体アプリ（Rust / eframe・egui）です。
```
 SDR#プラグイン ──TCP 7355(JSONL)──▶ jalert-receiver (ネイティブGUI)
                                          ├─ 表示: 待機 ↔ 全画面アラート
                                          ├─ 管理: 管理画面 (タブ)
                                          └─(任意) 内蔵Web + cloudflared ──▶ 遠隔から閲覧
```

> 受信元は SDR# プラグインの TCP JSONL のみです（衛星系/地上系の実機受信も対象外）。

- **単一実行ファイル**。フォント（M PLUS 1p）と Web UI を埋め込み済みで、
  追加ファイル・ランタイム不要。ダブルクリックで GUI 起動。
- ウィンドウGUI（winit + OpenGL）。`F11` でフルスクリーン、`Esc` で解除。

## 電文モデル

従来機の電文体系に合わせ、各電文は

- **電文種別コード** `alert_type`：`WRMA`(気象) / `IOEQ`(地震・震度速報) /
  `EPRQ`(緊急地震速報) / `ISSW`(津波) / `JALT`(試験・国民保護等)
- **情報種別** `alert_sub_type`：国民保護情報 / 緊急地震速報 / 地震情報 /
  震度速報 / 津波警報・注意報 / 火山情報 / 気象警報・注意報 / 試験・訓練
- **表示対象 `allowed`**：緊急情報表示設定（＝外部IF動作ルール）の対象か

として扱います。気象(WRMA)は従来通り JMA XML をパースし、`Body/Warning`
（府県予報区）の `<Kind>`＋`Status`（発表/継続/解除）から **特別警報 > 警報 >
注意報** を判定します。それ以外の情報種別は種別ごとに表示レベルが決まります
（国民保護＝最優先全画面、緊急地震速報/津波/火山＝全画面警報、地震情報/震度速報＝
情報バナー）。

## 画面

アプリ上部のタブで切り替えます。

### 🖥 表示（kiosk）
表示専用端末・電光掲示向け。視認性重視。

| モード | 条件 | 表示 |
|--------|------|------|
| 待機 | 表示対象の警報なし | 待機画面（下記3スタイル） |
| 情報 | 注意報・地震情報・震度速報など | 待機画面＋下部に黄色バナー |
| アラート | **警報級以上の表示対象** | 全画面 **赤**（種別・地域・本文・発表時刻、点滅） |
| アラート | **特別警報 / 国民保護** | 全画面 **紫** |

**待機画面スタイルは3種類**（設定で切替）:

- **シンプル**：時計＋「異常なし」のみ
- **パチモン**：J-ALERT ロゴ＋地球背景のオマージュ（手描き）
- **リアル**：**従来機の実画面（待機・アラート）をそのまま使用**。待機は実機の
  待機画像＋4情報ランプ点灯、アラートは情報種別ごとの実画面（国民保護／緊急地震速報・
  地震／津波／火山）に発表内容を重畳。気象は実画像が無いため色面表示にフォールバック。

> 「リアル」で使う待機/アラートの画像・チャイム・読み上げ音声は、従来機から
> 取り出した実アセットを同梱しています（中立名で格納）。

### 🔊 音声
アラート発生時に**実機のチャイム＋情報種別ごとの読み上げ**を再生します
（設定でON/OFF）。`audio` フィーチャ（既定ON）。音声デバイスが無い環境では自動的に無音。

### 🛠 管理
従来機の Web 管理画面（Rails）を**ネイティブGUIのタブ**として移植。
**ログイン**はユーザ種別＋パスワード（実機の `SystemConfig#authenticate` 準拠）。
初期パスワードは **システム管理者 `jl10ad` / 運用管理者 `opjl10` / 一般利用者 `usjl10`**。

- **トップ**：現在の状態・表示中の緊急情報サマリ
- **仮想パネル**：実機フロントパネル（リンク状態／Status／衛星系・地上系／アプリ／
  外部I/F ランプ＋接点出力DIO #1–8）を模した表示
- **緊急情報一覧**：全受信を時系列で既読/未読管理。情報種別・電文種別・受信系統・
  本文・**XML原文**を確認（再送電文は自動集約）
- **受信機状態**（運用管理者以上）：受信系統（衛星系・地上系の最終受信）・電文種別別受信数・検索
- **外部IF動作ルール**：情報種別ごとに「画面表示（緊急情報表示設定）／音声・鳴動／
  接点出力／同報系連携」を設定。**「画面表示」を外すとその種別は全画面化/バナーに
  出ません**（一覧には記録）
- **接続テスト**：受信元（SDR# プラグイン）への疎通確認
- **同報系I/F状態**（運用管理者以上）：同報系防災行政無線インタフェース状態（実機ハードが無いため模擬表示）

> 実機の Web は Rails＋SSE(`/observe`)＋外部I/Fデーモン群で構成されますが、本移植版は
> その全再現ではなく、画面・認証・電文分類・表示判定を実コードに準拠させたネイティブ実装です。

### ⚙ 設定（アプリ内）
上部の「⚙ 設定」から、再起動なしで変更できます。
- **JSONL 受信元の IP / ポート**（変更後に即再接続）
- **待機画面スタイル**：シンプル／パチモン／リアル
- **音声**：アラート時のチャイム・読み上げ ON/OFF
- **Web サーバ**：起動する/しない、**ポート**、cloudflared 公開の有無（その場で起動/停止）

### Web 配信（任意 / cloudflared）
設定で Web サーバを起動すると（または起動時 `--web` / `--cloudflared`）、ブラウザにも
配信します。
- `/` … **公開用 J-ALERT 表示画面**（待機 ↔ 全画面アラート）
- `/admin` … **管理用 緊急情報一覧**（既読/未読、ライト/ダーク）

cloudflared を使う場合は[各自インストール](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/)し、
設定で「cloudflared で外部公開」を有効にしてください。公開 URL はコンソール／
`jalert-receiver.log` に表示されます。

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

# 管理画面を内蔵Webでも公開し、cloudflared で外部からも閲覧
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
| `--web` | — | off | 管理/表示の内蔵 Web サーバを有効化 |
| `--web-port` | `JALERT_WEB_PORT` | `8080` | Web ポート |
| `--cloudflared` | `JALERT_CLOUDFLARED` | off | cloudflared クイックトンネルで外部公開（`--web` も自動有効化） |
| `--cloudflared-bin` | `JALERT_CLOUDFLARED_BIN` | `cloudflared` | cloudflared 実行ファイルのパス |

### テスト電文の再生（全カテゴリの確認）
SDR# プラグイン由来の JMA 気象 XML に加え、従来機形式の電文を直接 JSONL で
投入できます（`--replay` で利用）。1行=1電文の例:

```json
{"decoded":true,"alert_type":"EPRQ","alert_sub_type":"緊急地震速報","info_type":"発表","headline":"強い揺れに警戒してください","channel":"衛星系","rx_time_ms":0}
{"decoded":true,"alert_type":"JALT","alert_sub_type":"国民保護情報","info_type":"発表","headline":"ミサイル発射情報","channel":"地上系","rx_time_ms":0}
{"decoded":true,"alert_type":"IOEQ","alert_sub_type":"震度速報","info_type":"発表","headline":"最大震度4","rx_time_ms":0}
```

## ビルド

[Rust](https://rustup.rs/)（stable）が必要です。

```sh
cargo build --release               # GUI 版（既定）
cargo test  --no-default-features    # コアの単体テスト（GUI 抜き）
cargo run   -- --replay jalert_test_data/decoded.jsonl --replay-interval 300
```

`--no-default-features` でビルドすると **GUI 抜きのヘッドレス版**（内蔵 Web サーバ
のみ）になります。サーバ常駐用途に。

### Linux のビルド依存
eframe(winit/glow) のため、以下が必要です：

```sh
sudo apt-get install -y libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev \
  libxcb-xfixes0-dev libxkbcommon-dev libssl-dev libgl1-mesa-dev libasound2-dev
```

`libasound2-dev` は音声(`audio` フィーチャ, rodio/ALSA)用です。音声不要なら
`--no-default-features --features gui` でビルドすれば ALSA 依存なしになります。

## GPUの無い環境（VM / RDP）での表示

GPU/3Dドライバの無い環境（Proxmox 等の VM、RDP）では OpenGL も Vulkan/DX12 も
利用できず GUI が起動しないことがあります。Windows 版 zip には **Mesa3D の
ソフトウェア OpenGL（`opengl32.dll`）を同梱**しており、exe と同じフォルダに置く
ことでソフトウェア描画で起動します。起動順序は **glow(OpenGL) →
wgpu(DX12/Vulkan/WARP)** で自動選択し、失敗内容は exe 隣の
`jalert-receiver.log` に記録されます。

## 配布 / Release
タグ（`v*`）を push すると GitHub Actions が Linux / Windows のネイティブ実行
ファイルをビルドし、Release に成果物を添付します
（[.github/workflows/release.yml](.github/workflows/release.yml)）。

## ライセンス / クレジット
- 同梱フォント: **M PLUS 1p**（SIL Open Font License 1.1）。
- 本体: MIT（[LICENSE](LICENSE)）。
