//! Optional embedded web server (for remote viewing via Cloudflare Tunnel) plus
//! cloudflared integration. Serves the mailbox UI and a small JSON/XML API. The
//! page polls `/api/state`; this keeps the server dependency-light (tiny_http).

use crate::model::{InboxItem, Severity};
use crate::state::AppState;
use std::io::{BufRead, BufReader, Read};
use std::sync::{Arc, Mutex};

const INBOX_HTML: &str = include_str!("../wwwroot/inbox.html");

#[derive(Clone)]
pub struct WebConfig {
    pub port: u16,
    pub cloudflared: bool,
    pub cloudflared_bin: String,
}

pub fn spawn(cfg: WebConfig, state: Arc<Mutex<AppState>>) {
    std::thread::Builder::new()
        .name("jalert-web".into())
        .spawn(move || serve(cfg, state))
        .expect("spawn web thread");
}

fn serve(cfg: WebConfig, state: Arc<Mutex<AppState>>) {
    let addr = format!("0.0.0.0:{}", cfg.port);
    let server = match tiny_http::Server::http(&addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[web] cannot listen on {}: {}", addr, e);
            return;
        }
    };
    eprintln!("[web] listening on http://localhost:{}/  (受信箱)", cfg.port);

    if cfg.cloudflared {
        start_cloudflared(&cfg.cloudflared_bin, cfg.port);
    }

    for mut req in server.incoming_requests() {
        let (path, query) = split_url(req.url());
        let resp = route(&path, &query, &mut req, &state);
        let _ = req.respond(resp);
    }
}

fn route(
    path: &str,
    query: &str,
    req: &mut tiny_http::Request,
    state: &Arc<Mutex<AppState>>,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    match path {
        "/" | "/inbox" => html(INBOX_HTML),
        "/healthz" => text("ok"),
        "/api/state" => json(state_json(&state.lock().unwrap())),
        "/api/xml" => {
            let id = param(query, "id").and_then(|v| v.parse::<u64>().ok());
            match id.and_then(|id| state.lock().unwrap().item(id).map(|i| i.xml.clone())) {
                Some(xml) => with_type(xml.into_bytes(), "application/xml; charset=utf-8"),
                None => not_found(),
            }
        }
        "/api/read" => {
            // Drain any request body (ignored) so the socket stays healthy.
            let mut sink = Vec::new();
            let _ = req.as_reader().read_to_end(&mut sink);
            let mut st = state.lock().unwrap();
            if param(query, "all").as_deref() == Some("true") {
                st.mark_all_read();
            } else if let Some(id) = param(query, "id").and_then(|v| v.parse::<u64>().ok()) {
                st.mark_read(id, param(query, "read").as_deref() != Some("false"));
            }
            text("ok")
        }
        _ => not_found(),
    }
}

fn state_json(st: &AppState) -> serde_json::Value {
    let r = &st.receiver;
    let inbox: Vec<serde_json::Value> = st.inbox().map(item_json).collect();
    serde_json::json!({
        "mode": st.mode(),
        "unread": st.unread(),
        "inbox": inbox,
        "receiver": {
            "connected": r.connected,
            "source": r.source,
            "totalLines": r.total_lines,
            "lastLineMs": r.last_line_ms,
        },
    })
}

fn item_json(it: &InboxItem) -> serde_json::Value {
    serde_json::json!({
        "id": it.id,
        "rxTimeMs": it.rx_time_ms,
        "packetTime": it.packet_time,
        "severity": sev_num(it.severity),
        "severityLabel": it.severity.label(),
        "infoType": it.info_type,
        "title": it.title,
        "headTitle": it.head_title,
        "areaName": it.area_name,
        "kinds": it.kinds,
        "headline": it.headline,
        "reportTime": it.report_time,
        "read": it.read,
    })
}

fn sev_num(s: Severity) -> u8 {
    match s {
        Severity::None => 0,
        Severity::Advisory => 1,
        Severity::Warning => 2,
        Severity::Emergency => 3,
    }
}

// ---- cloudflared ----

fn start_cloudflared(bin: &str, port: u16) {
    use std::process::{Command, Stdio};
    let child = Command::new(bin)
        .args(["tunnel", "--no-autoupdate", "--url", &format!("http://localhost:{port}")])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[cloudflared] could not start '{}': {}", bin, e);
            eprintln!("[cloudflared] install: https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/");
            return;
        }
    };
    eprintln!("[cloudflared] starting quick tunnel for http://localhost:{port} …");
    for stream in [child.stdout.take().map(scan), child.stderr.take().map(scan)]
        .into_iter()
        .flatten()
    {
        drop(stream); // threads detached inside scan()
    }
    // Keep the child alive for the process lifetime.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

fn scan<R: Read + Send + 'static>(r: R) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for line in BufReader::new(r).lines().map_while(Result::ok) {
            if let Some(url) = find_tunnel_url(&line) {
                eprintln!("\n========================================");
                eprintln!("  公開URL (Cloudflare Tunnel):\n  {url}");
                eprintln!("========================================\n");
            }
        }
    })
}

fn find_tunnel_url(line: &str) -> Option<String> {
    let start = line.find("https://")?;
    let rest = &line[start..];
    let end = rest.find(".trycloudflare.com")? + ".trycloudflare.com".len();
    Some(rest[..end].to_string())
}

// ---- tiny_http helpers ----

type Resp = tiny_http::Response<std::io::Cursor<Vec<u8>>>;

fn with_type(body: Vec<u8>, content_type: &str) -> Resp {
    let header = tiny_http::Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap();
    tiny_http::Response::from_data(body).with_header(header)
}
fn html(s: &str) -> Resp {
    with_type(s.as_bytes().to_vec(), "text/html; charset=utf-8")
}
fn text(s: &str) -> Resp {
    with_type(s.as_bytes().to_vec(), "text/plain; charset=utf-8")
}
fn json(v: serde_json::Value) -> Resp {
    with_type(v.to_string().into_bytes(), "application/json; charset=utf-8")
}
fn not_found() -> Resp {
    tiny_http::Response::from_data(b"not found".to_vec()).with_status_code(404)
}

fn split_url(url: &str) -> (String, String) {
    match url.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (url.to_string(), String::new()),
    }
}

fn param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        if k == key {
            Some(v.to_string())
        } else {
            None
        }
    })
}
