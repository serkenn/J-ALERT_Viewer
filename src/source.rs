//! Upstream feed: connect to the SDR# plugin's TCP JSONL sink (default
//! 127.0.0.1:7355) and read newline-delimited records, reconnecting forever; or
//! replay a `.jsonl` file for testing.
//!
//! The endpoint can be changed at runtime via [`SourceCtl::set_endpoint`]; the
//! reader thread drops the current connection and reconnects to the new target.

use crate::classify::from_json_line;
use crate::state::AppState;
use std::io::{BufRead, BufReader};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
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

/// Live control over the upstream endpoint, shared with the UI.
pub struct SourceCtl {
    ep: Mutex<(String, u16)>,
    generation: AtomicU64,
    stream: Mutex<Option<TcpStream>>, // current connection, for shutdown
    replay: Option<String>,
    state: Arc<Mutex<AppState>>,
}

impl SourceCtl {
    pub fn endpoint(&self) -> (String, u16) {
        self.ep.lock().unwrap().clone()
    }

    pub fn is_replay(&self) -> bool {
        self.replay.is_some()
    }

    /// Point the feed at a new host:port and force an immediate reconnect.
    pub fn set_endpoint(&self, host: String, port: u16) {
        {
            let mut e = self.ep.lock().unwrap();
            if *e == (host.clone(), port) {
                return;
            }
            *e = (host.clone(), port);
        }
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.state.lock().unwrap().receiver.source = format!("{host}:{port}");
        if let Some(s) = self.stream.lock().unwrap().as_ref() {
            let _ = s.shutdown(Shutdown::Both); // unblock the reader -> reconnect
        }
    }
}

pub fn spawn(cfg: SourceConfig, state: Arc<Mutex<AppState>>, on_change: OnChange) -> Arc<SourceCtl> {
    let ctl = Arc::new(SourceCtl {
        ep: Mutex::new((cfg.host.clone(), cfg.port)),
        generation: AtomicU64::new(0),
        stream: Mutex::new(None),
        replay: cfg.replay.clone(),
        state: state.clone(),
    });
    let ctl_thread = ctl.clone();
    let interval = cfg.replay_interval_ms;
    std::thread::Builder::new()
        .name("jalert-source".into())
        .spawn(move || run(ctl_thread, interval, state, on_change))
        .expect("spawn source thread");
    ctl
}

fn run(ctl: Arc<SourceCtl>, interval_ms: u64, state: Arc<Mutex<AppState>>, on_change: OnChange) {
    if let Some(path) = ctl.replay.clone() {
        replay(&path, interval_ms, &state, &on_change);
        return;
    }
    loop {
        let (host, port) = ctl.endpoint();
        let gen_at_start = ctl.generation.load(Ordering::SeqCst);

        match TcpStream::connect((host.as_str(), port)) {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                *ctl.stream.lock().unwrap() = stream.try_clone().ok();
                set_connected(&state, true, &on_change);
                eprintln!("[tcp] connected to {host}:{port}");
                read_lines(stream, &state, &on_change);
                *ctl.stream.lock().unwrap() = None;
                set_connected(&state, false, &on_change);
                eprintln!("[tcp] disconnected from {host}:{port}");
            }
            Err(e) => {
                set_connected(&state, false, &on_change);
                eprintln!("[tcp] {host}:{port}: {e}");
            }
        }
        // Reconnect immediately if the endpoint changed; otherwise back off.
        if ctl.generation.load(Ordering::SeqCst) == gen_at_start {
            std::thread::sleep(Duration::from_secs(2));
        }
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
            eprintln!("[replay] {path}: {e}");
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
