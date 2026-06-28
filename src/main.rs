// J-ALERT native receiver/display. Connects to the SDR# plugin's TCP JSONL sink
// and shows a kiosk-style display (待機 ↔ 全画面アラート) plus a mailbox-style
// management view. An optional embedded web server + cloudflared exposes the
// mailbox remotely.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use jalert_receiver::source::SourceConfig;
use jalert_receiver::state::AppState;
use jalert_receiver::web::WebConfig;
use std::sync::{Arc, Mutex};

#[cfg(feature = "gui")]
mod ui;

struct Config {
    source_host: String,
    source_port: u16,
    web_port: u16,
    web_enabled: bool,
    replay: Option<String>,
    replay_interval: u64,
    cloudflared: bool,
    cloudflared_bin: String,
    fullscreen: bool,
}

impl Config {
    fn parse() -> Config {
        let env = |k: &str, d: &str| std::env::var(k).ok().filter(|s| !s.is_empty()).unwrap_or_else(|| d.to_string());
        let mut c = Config {
            source_host: env("JALERT_SOURCE_HOST", "127.0.0.1"),
            source_port: env("JALERT_SOURCE_PORT", "7355").parse().unwrap_or(7355),
            web_port: env("JALERT_WEB_PORT", "8080").parse().unwrap_or(8080),
            web_enabled: false,
            replay: None,
            replay_interval: 800,
            cloudflared: matches!(env("JALERT_CLOUDFLARED", "").as_str(), "1" | "true"),
            cloudflared_bin: env("JALERT_CLOUDFLARED_BIN", "cloudflared"),
            fullscreen: false,
        };
        let mut args = std::env::args().skip(1);
        while let Some(a) = args.next() {
            let mut val = || args.next().expect("missing value");
            match a.as_str() {
                "--source-host" => c.source_host = val(),
                "--source-port" => c.source_port = val().parse().unwrap_or(7355),
                "--web-port" => c.web_port = val().parse().unwrap_or(8080),
                "--web" => c.web_enabled = true,
                "--replay" => c.replay = Some(val()),
                "--replay-interval" => c.replay_interval = val().parse().unwrap_or(800),
                "--cloudflared" => { c.cloudflared = true; c.web_enabled = true; }
                "--cloudflared-bin" => c.cloudflared_bin = val(),
                "--fullscreen" => c.fullscreen = true,
                "-h" | "--help" => { print_help(); std::process::exit(0); }
                other => eprintln!("unknown argument: {other}"),
            }
        }
        if c.cloudflared {
            c.web_enabled = true;
        }
        c
    }
}

fn print_help() {
    println!(
        "jalert-receiver — J-ALERT 受信表示\n\n\
         --source-host H        プラグインの TCP ホスト (既定 127.0.0.1)\n\
         --source-port P        プラグインの TCP JSONL ポート (既定 7355)\n\
         --replay FILE          JSONL を再生 (テスト用)\n\
         --replay-interval MS   再生間隔 (既定 800)\n\
         --web                  受信箱の内蔵 Web サーバを有効化\n\
         --web-port P           Web ポート (既定 8080)\n\
         --cloudflared          cloudflared クイックトンネルで外部公開\n\
         --cloudflared-bin PATH cloudflared 実行ファイルのパス\n\
         --fullscreen           起動時にフルスクリーン表示"
    );
}

/// Common setup: parse config, build shared state, start the web server.
/// Returns the config, shared state and the source-feed configuration.
fn setup() -> (Config, Arc<Mutex<AppState>>, SourceConfig) {
    let cfg = Config::parse();
    let source = match &cfg.replay {
        Some(f) => format!("replay:{}", f),
        None => format!("{}:{}", cfg.source_host, cfg.source_port),
    };
    let state = Arc::new(Mutex::new(AppState::new(source)));

    if cfg.web_enabled {
        jalert_receiver::web::spawn(
            WebConfig { port: cfg.web_port, cloudflared: cfg.cloudflared, cloudflared_bin: cfg.cloudflared_bin.clone() },
            state.clone(),
        );
    }

    let src_cfg = SourceConfig {
        host: cfg.source_host.clone(),
        port: cfg.source_port,
        replay: cfg.replay.clone(),
        replay_interval_ms: cfg.replay_interval,
    };
    (cfg, state, src_cfg)
}

#[cfg(feature = "gui")]
fn main() -> eframe::Result<()> {
    let (cfg, state, src_cfg) = setup();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("J-ALERT 受信表示")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([720.0, 480.0])
            .with_fullscreen(cfg.fullscreen),
        ..Default::default()
    };
    let fullscreen = cfg.fullscreen;
    eframe::run_native(
        "jalert-receiver",
        options,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, state, src_cfg, fullscreen)))),
    )
}

// Headless build (`--no-default-features`): web server only, no window.
#[cfg(not(feature = "gui"))]
fn main() {
    let (_cfg, state, src_cfg) = setup();
    jalert_receiver::source::spawn(src_cfg, state, Arc::new(|| {}));
    eprintln!("[headless] running web server only; Ctrl+C to quit.");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
