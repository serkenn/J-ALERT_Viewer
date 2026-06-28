//! Shared application state: the live warning channels that drive the display,
//! plus a mailbox-style history of every received telegram with read/unread
//! flags. Guarded by a `Mutex`; the UI reads it each frame and the receiver
//! thread writes to it.

use crate::model::{AlertChannel, InboxItem, ReceiverStatus, Severity};
use chrono::Utc;
use std::collections::HashMap;

const HISTORY_CAP: usize = 500;

#[derive(Default)]
pub struct AppState {
    channels: HashMap<String, AlertChannel>,
    history: Vec<InboxItem>, // oldest first
    next_id: u64,
    pub receiver: ReceiverStatus,
}

impl AppState {
    pub fn new(source: String) -> Self {
        AppState {
            receiver: ReceiverStatus { source, ..Default::default() },
            next_id: 1,
            ..Default::default()
        }
    }

    /// Apply one decoded line: record it in the mailbox and update the channels.
    pub fn ingest(&mut self, ch: AlertChannel) {
        self.receiver.last_line_ms = Utc::now().timestamp_millis();
        self.receiver.total_lines += 1;

        self.record_history(&ch);

        if ch.severity == Severity::None {
            self.channels.remove(&ch.key);
        } else {
            self.channels.insert(ch.key.clone(), ch);
        }
    }

    pub fn set_connected(&mut self, connected: bool) {
        self.receiver.connected = connected;
    }

    fn record_history(&mut self, ch: &AlertChannel) {
        let kinds: Vec<String> = ch.kinds.iter().map(|k| k.name.clone()).collect();

        // Collapse retransmits: the plugin re-sends a telegram every few seconds.
        if let Some(prev) = self.history.iter_mut().rev().find(|h| h.head_title == ch.head_title) {
            if prev.severity == ch.severity
                && prev.info_type == ch.info_type
                && prev.headline == ch.headline
                && prev.kinds == kinds
            {
                prev.rx_time_ms = ch.rx_time_ms;
                prev.packet_time = ch.packet_time.clone();
                return;
            }
        }

        let item = InboxItem {
            id: self.next_id,
            rx_time_ms: ch.rx_time_ms,
            packet_time: ch.packet_time.clone(),
            severity: ch.severity,
            info_type: ch.info_type.clone(),
            title: ch.title.clone(),
            head_title: ch.head_title.clone(),
            area_name: ch.area_name.clone(),
            kinds,
            headline: ch.headline.clone(),
            report_time: ch.report_time.clone(),
            read: false,
            xml: ch.xml.clone(),
        };
        self.next_id += 1;
        self.history.push(item);
        if self.history.len() > HISTORY_CAP {
            let drop = self.history.len() - HISTORY_CAP;
            self.history.drain(0..drop);
        }
    }

    // ---- queries used by the UI / web layer ----

    pub fn top_severity(&self) -> Severity {
        self.channels.values().map(|c| c.severity).max().unwrap_or(Severity::None)
    }

    /// "standby" | "advisory" | "alert"
    pub fn mode(&self) -> &'static str {
        match self.top_severity() {
            Severity::Emergency | Severity::Warning => "alert",
            Severity::Advisory => "advisory",
            Severity::None => "standby",
        }
    }

    /// 警報・特別警報, most severe then most recent first.
    pub fn alerts(&self) -> Vec<&AlertChannel> {
        let mut v: Vec<&AlertChannel> =
            self.channels.values().filter(|c| c.severity >= Severity::Warning).collect();
        v.sort_by(|a, b| b.severity.cmp(&a.severity).then(b.rx_time_ms.cmp(&a.rx_time_ms)));
        v
    }

    pub fn advisories(&self) -> Vec<&AlertChannel> {
        let mut v: Vec<&AlertChannel> =
            self.channels.values().filter(|c| c.severity == Severity::Advisory).collect();
        v.sort_by(|a, b| b.rx_time_ms.cmp(&a.rx_time_ms));
        v
    }

    pub fn primary(&self) -> Option<&AlertChannel> {
        self.alerts().into_iter().next()
    }

    /// Mailbox history, newest first.
    pub fn inbox(&self) -> impl Iterator<Item = &InboxItem> {
        self.history.iter().rev()
    }

    pub fn unread(&self) -> usize {
        self.history.iter().filter(|h| !h.read).count()
    }

    pub fn item(&self, id: u64) -> Option<&InboxItem> {
        self.history.iter().find(|h| h.id == id)
    }

    pub fn mark_read(&mut self, id: u64, read: bool) {
        if let Some(it) = self.history.iter_mut().find(|h| h.id == id) {
            it.read = read;
        }
    }

    pub fn mark_all_read(&mut self) {
        for h in &mut self.history {
            h.read = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::from_json_line;

    fn line(area: &str, kind: &str, status: &str) -> String {
        let xml = format!(
            "<Report xmlns=\"http://x/\"><Body xmlns=\"http://x/c/\">\
             <Warning type=\"気象警報・注意報（府県予報区等）\"><Item>\
             <Kind><Name>{kind}</Name><Status>{status}</Status></Kind>\
             <Area><Name>{area}</Name></Area></Item></Warning></Body></Report>"
        );
        serde_json::json!({
            "decoded": true, "rx_time_ms": 1i64, "head_title": format!("{area}気象警報・注意報"),
            "info_type": "発表", "headline": "x", "xml": xml,
        })
        .to_string()
    }

    #[test]
    fn alert_then_clear_returns_to_standby() {
        let mut st = AppState::new("test".into());
        st.ingest(from_json_line(&line("東京都", "大雨警報", "発表")).unwrap());
        assert_eq!(st.mode(), "alert");
        st.ingest(from_json_line(&line("東京都", "大雨警報", "解除")).unwrap());
        assert_eq!(st.mode(), "standby");
    }

    #[test]
    fn retransmit_does_not_duplicate_inbox() {
        let mut st = AppState::new("test".into());
        st.ingest(from_json_line(&line("東京都", "大雨警報", "発表")).unwrap());
        st.ingest(from_json_line(&line("東京都", "大雨警報", "発表")).unwrap());
        assert_eq!(st.inbox().count(), 1);
    }

    #[test]
    fn read_tracking() {
        let mut st = AppState::new("test".into());
        st.ingest(from_json_line(&line("東京都", "大雨警報", "発表")).unwrap());
        assert_eq!(st.unread(), 1);
        let id = st.inbox().next().unwrap().id;
        st.mark_read(id, true);
        assert_eq!(st.unread(), 0);
    }
}
