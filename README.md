# J-ALERT Viewer

[SDR# J-Alert デコーダプラグイン](https://github.com/serkenn) が TCP で配信する
JSONL アラートを購読し、ブラウザに **J-ALERT 受信機ふう**の常時表示を出す
単体アプリ（.NET / 依存なし）です。jars2000 等の自動起動受信機のような
「待機画面 ↔ 全画面アラート」の振る舞いを Web で再現します。

```
 SDR#プラグイン ──TCP 7355(JSONL)──▶ jalert-receiver ──HTTP/SSE──▶ ブラウザ表示
```

## 仕組み

- プラグインの **TCP JSONL シンク**（既定 `127.0.0.1:7355`）へ接続し、自動再接続。
- 各行の全文 JMA XML をパースし、`Body/Warning`（府県予報区）の
  `<Kind>` の `Name`＋`Status`（発表/継続/解除）から重大度を判定。
  - **特別警報 > 警報 > 注意報**。`解除` の種別は失効として扱う。
- 表示ポリシー：**警報・特別警報のみ全画面アラート**。注意報は待機画面下部の
  控えめなバナーに表示。対象区域の全種別が解除されたら待機画面へ復帰。
- ブラウザへは **Server-Sent Events** で状態をライブ push。

## 2 つの画面

| URL | 用途 | デザイン |
|-----|------|----------|
| `/` | **表示画面 (kiosk)** … 表示専用端末・電光掲示向け。待機 ↔ 全画面アラート | 視認性重視のダーク／警報音 |
| `/inbox` | **受信箱 (管理)** … メールボックス風に全受信を既読/未読管理 | デジタル庁デザインシステム準拠・ライト/ダーク両対応 |

受信箱は受信した全電文を時系列で一覧し、**既読/未読**を**サーバ側で保持**（どの
端末から開いても共有）。行をクリックで既読化・本文・発表種別・XML 原文を確認でき、
「すべて既読」やテーマ切替（システム追従／ライト／ダーク）も可能。再送電文は
自動的に集約して重複表示しません。

## 表示画面のモード

| モード | 条件 | 表示 |
|--------|------|------|
| 待機 (standby) | 警報・注意報なし | 時計・日付・「受信待機中／異常なし」＋受信機ステータス |
| 注意報 (advisory) | 注意報のみ | 待機画面＋下部に黄色の注意報バナー |
| アラート (alert) | 警報 | 全画面 **赤**（種別・地域・本文・発表時刻、警報音） |
| アラート (alert) | 特別警報 | 全画面 **紫**（同上、警報音を強調パターンに） |

複数地域が同時発表のときは最も重大／最新を全画面に、残りはフッタに一覧表示。
警報音はブラウザの自動再生制限のため、画面右上の「🔔 音声」で一度有効化します。

## 実行

```sh
# 通常（プラグインの TCP シンクへ接続）
dotnet run -c Release

# 接続先・ポート指定
dotnet run -c Release -- --source-host 127.0.0.1 --source-port 7355 --web-port 8080

# プラグインなしで動作確認（JSONL を再生）
dotnet run -c Release -- --replay path/to/decoded.jsonl --replay-interval 800

# Cloudflare Tunnel で外部公開（受信箱もインターネットから閲覧可）
dotnet run -c Release -- --cloudflared
```

表示画面 `http://localhost:8080/`／受信箱 `http://localhost:8080/inbox` を開く。
表示専用端末ではキオスク全画面推奨。

### Cloudflared 対応

`--cloudflared`（または `JALERT_CLOUDFLARED=1`）を付けると、起動時に
[`cloudflared`](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/)
のクイックトンネルを Web ポートの前段に立ち上げ、`https://xxxx.trycloudflare.com`
形式の**公開 URL をコンソールに表示**します。ファイアウォールのポート開放なしに
受信箱・表示画面をインターネット越しに閲覧できます（`cloudflared` を PATH に置くか
`--cloudflared-bin` でパス指定）。常設運用では名前付きトンネルの利用を推奨。

### オプション / 環境変数

| フラグ | 環境変数 | 既定 | 説明 |
|--------|----------|------|------|
| `--source-host` | `JALERT_SOURCE_HOST` | `127.0.0.1` | プラグインの TCP ホスト |
| `--source-port` | `JALERT_SOURCE_PORT` | `7355` | プラグインの TCP JSONL ポート |
| `--web-port` | `JALERT_WEB_PORT` | `8080` | Web UI のポート |
| `--replay FILE` | — | — | JSONL を再生（テスト用） |
| `--replay-interval` | — | `800` | 再生間隔(ms) |
| `--cloudflared` | `JALERT_CLOUDFLARED` | off | 起動時に cloudflared クイックトンネルを開始 |
| `--cloudflared-bin` | `JALERT_CLOUDFLARED_BIN` | `cloudflared` | cloudflared 実行ファイルのパス |

### エンドポイント

- `GET /` … 表示画面（kiosk・単一ページ）
- `GET /inbox` … 受信箱（管理 UI）
- `GET /events` … 状態の SSE ストリーム
- `GET /api/state` … 現在状態の JSON（ワンショット）
- `GET /api/xml?id=<n>` / `?key=<head_title>` … JMA XML 原文
- `POST /api/read?id=<n>&read=true|false` / `?all=true` … 既読/未読の更新
- `GET /healthz` … 死活確認

## ビルド / 配布

成果物は **自己完結（self-contained）単一ファイル**。ランタイム不要で動きます。

```sh
# Linux x64
dotnet publish -c Release -r linux-x64

# Windows x64
dotnet publish -c Release -r win-x64
```

タグ（`v*`）を push すると GitHub Actions が linux-x64 / win-x64 をビルドし、
Release に成果物を添付します（[.github/workflows/release.yml](.github/workflows/release.yml)）。

> Windows で `+:PORT` への bind に失敗する場合は管理者権限、または
> `netsh http add urlacl url=http://+:8080/ user=Everyone` を実行してください
> （未指定時は自動で `localhost` のみへフォールバックします）。

## 入力フォーマット

プラグインが出す JSONL の各行は 1 アラート（再結合済みファイル 1 件）で、
`decoded` / `chunk_type` / `head_title` / `info_type` / `headline` などのメタと、
復号できた場合は全文 `xml` を含みます。詳細はプラグイン同梱の
`docs/JSONL_FORMAT.ja.md` を参照。
