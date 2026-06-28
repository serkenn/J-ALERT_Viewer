//! Upstream feed: connect to the SDR# plugin's TCP JSONL sink (default
//! 127.0.0.1:7355) and read newline-delimited records, reconnecting forever; or
//! replay a `.jsonl` file for testing. Runs on its own thread and pokes the UI
//! via the `on_change` callback after each ingested record.

use crate::classify::from_json_line;
use crate::state::AppState;
use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone)]
pub struct SourceConfig {
    pub host: String,
    pub port: u16,
    pub replay: Option<String>,
    pub replay_interval_ms: u64,
}

pub type OnChange = Arc<dyn Fn() + Send + Sync>;

pub fn spawn(cfg: SourceConfig, state: Arc<Mutex<AppState>>, on_change: OnChange) {
    std::thread::Builder::new()
        .name("jalert-source".into())
        .spawn(move || run(cfg, state, on_change))
        .expect("spawn source thread");
}

fn run(cfg: SourceConfig, state: Arc<Mutex<AppState>>, on_change: OnChange) {
    if let Some(path) = cfg.replay.clone() {
        replay(&path, cfg.replay_interval_ms, &state, &on_change);
        return;
    }
    loop {
        match TcpStream::connect((cfg.host.as_str(), cfg.port)) {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                set_connected(&state, true, &on_change);
                eprintln!("[tcp] connected to {}:{}", cfg.host, cfg.port);
                read_lines(stream, &state, &on_change);
                set_connected(&state, false, &on_change);
                eprintln!("[tcp] disconnected");
            }
            Err(e) => {
                set_connected(&state, false, &on_change);
                eprintln!("[tcp] {}: {}", cfg.host, e);
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn read_lines(stream: TcpStream, state: &Arc<Mutex<AppState>>, on_change: &OnChange) {
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        match line {
            Ok(l) => ingest(&l, state, on_change),
            Err(_) => break,
        }
    }
}

fn replay(path: &str, interval_ms: u64, state: &Arc<Mutex<AppState>>, on_change: &OnChange) {
    set_connected(state, true, on_change);
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[replay] {}: {}", path, e);
            return;
        }
    };
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        ingest(line, state, on_change);
        std::thread::sleep(Duration::from_millis(interval_ms));
    }
    eprintln!("[replay] done");
}

fn ingest(line: &str, state: &Arc<Mutex<AppState>>, on_change: &OnChange) {
    if let Some(ch) = from_json_line(line) {
        state.lock().unwrap().ingest(ch);
        on_change();
    }
}

fn set_connected(state: &Arc<Mutex<AppState>>, connected: bool, on_change: &OnChange) {
    state.lock().unwrap().set_connected(connected);
    on_change();
}
