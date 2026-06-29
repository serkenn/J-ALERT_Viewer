//! Shared application state: the live warning channels that drive the display, a
//! mailbox-style history of every received telegram, plus the supporting state
//! the ported legacy management screens read/write (外部インタフェース動作
//! ルール, システム状態, 接続テスト, 同報系I/F 状態).
//!
//! Guarded by a `Mutex`; the UI reads it each frame and the receiver thread
//! writes to it.

use crate::model::{
    AlertChannel, AlertType, Category, InboxItem, LampGroup, ReceiverStatus, RxChannel, Severity,
};
use chrono::Utc;
use std::collections::HashMap;

const HISTORY_CAP: usize = 500;

/// One row of the 外部インタフェース動作ルール table. `display` doubles as the
/// 緊急情報表示設定 (whether a telegram of this category reaches the screen); the
/// rest mirror the external actions the legacy receiver can fire.
#[derive(Clone, Debug)]
pub struct InterfaceRule {
    pub category: Category,
    pub alert_type: AlertType,
    pub display: bool,     // 緊急情報表示（画面表示の対象）
    pub sound: bool,       // 音声合成・鳴動
    pub contact_out: bool, // 接点出力
    pub cwsd: bool,        // 同報系連携
}

/// A simulated broadcast-link (同報系防災行政無線) status, mirroring the fields
/// the legacy receiver logs. There is no real hardware in this port, so it is static.
#[derive(Clone, Debug)]
pub struct CwsdStatus {
    pub host: String,
    pub local_no: String,
    pub remote_no: String,
    pub command: String,
    pub queue_len: u32,
    pub connected: bool,
    pub checksum: String,
}

impl Default for CwsdStatus {
    fn default() -> Self {
        CwsdStatus {
            host: "192.168.200.121#3399".into(),
            local_no: "11".into(),
            remote_no: "01".into(),
            command: "20".into(),
            queue_len: 0,
            connected: false, // no real hardware in this port
            checksum: "----".into(),
        }
    }
}

/// Result of a 接続テスト run.
#[derive(Clone, Debug)]
pub struct ConnectTestResult {
    pub at_ms: i64,
    pub ok: bool,
    pub message: String,
}

pub struct AppState {
    channels: HashMap<String, AlertChannel>,
    history: Vec<InboxItem>, // oldest first
    next_id: u64,
    pub receiver: ReceiverStatus,
    pub rules: Vec<InterfaceRule>,
    pub cwsd: CwsdStatus,
    pub connect_test: Option<ConnectTestResult>,
    // per-channel last receive + per-type counts, for the システム状態 screen
    pub last_sat_ms: i64,
    pub last_terr_ms: i64,
    type_counts: HashMap<&'static str, u64>,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            channels: HashMap::new(),
            history: Vec::new(),
            next_id: 1,
            receiver: ReceiverStatus::default(),
            rules: default_rules(),
            cwsd: CwsdStatus::default(),
            connect_test: None,
            last_sat_ms: 0,
            last_terr_ms: 0,
            type_counts: HashMap::new(),
        }
    }
}

/// The seed 外部インタフェース動作ルール, ordered by display priority. Display is
/// on for everything by default; an operator can switch a category off so it no
/// longer takes over the kiosk.
fn default_rules() -> Vec<InterfaceRule> {
    use Category::*;
    let row = |category: Category, alert_type: AlertType, display, sound, contact_out, cwsd| InterfaceRule {
        category,
        alert_type,
        display,
        sound,
        contact_out,
        cwsd,
    };
    vec![
        row(CivilProtection, AlertType::Jalt, true, true, true, true),
        row(Eew, AlertType::Eprq, true, true, true, true),
        row(Tsunami, AlertType::Issw, true, true, true, true),
        row(Volcano, AlertType::Volc, true, true, false, true),
        row(EmergencyContact, AlertType::Ifda, true, true, false, true),
        row(Earthquake, AlertType::Ioeq, true, false, false, false),
        row(SeismicIntensity, AlertType::Ioeq, true, false, false, false),
        row(Weather, AlertType::Wrma, true, false, false, false),
        row(Test, AlertType::Jalt, true, false, false, false),
        row(Other, AlertType::Unknown, true, false, false, false),
    ]
}

impl AppState {
    pub fn new(source: String) -> Self {
        AppState {
            receiver: ReceiverStatus { source, ..Default::default() },
            ..Default::default()
        }
    }

    fn rule_for(&self, category: Category) -> Option<&InterfaceRule> {
        self.rules.iter().find(|r| r.category == category)
    }

    /// Apply one decoded line: record it in the mailbox and update the channels.
    pub fn ingest(&mut self, mut ch: AlertChannel) {
        self.receiver.last_line_ms = Utc::now().timestamp_millis();
        self.receiver.total_lines += 1;
        *self.type_counts.entry(ch.alert_type.code()).or_insert(0) += 1;
        match ch.channel {
            RxChannel::Satellite => self.last_sat_ms = self.receiver.last_line_ms,
            RxChannel::Terrestrial => self.last_terr_ms = self.receiver.last_line_ms,
            RxChannel::Unknown => {}
        }

        // The 緊急情報表示設定 (rule.display) decides whether it reaches the screen.
        ch.allowed = self.rule_for(ch.category).map_or(true, |r| r.display);

        self.record_history(&ch);

        if ch.effective_severity() == Severity::None {
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

        // Collapse retransmits: the same telegram arrives every few seconds and
        // on both channels.
        if let Some(prev) = self.history.iter_mut().rev().find(|h| h.head_title == ch.head_title && h.category == ch.category) {
            if prev.severity == ch.severity
                && prev.info_type == ch.info_type
                && prev.headline == ch.headline
                && prev.kinds == kinds
            {
                prev.rx_time_ms = ch.rx_time_ms;
                prev.packet_time = ch.packet_time.clone();
                prev.channel = ch.channel;
                return;
            }
        }

        let item = InboxItem {
            id: self.next_id,
            rx_time_ms: ch.rx_time_ms,
            packet_time: ch.packet_time.clone(),
            alert_type: ch.alert_type,
            category: ch.category,
            allowed: ch.allowed,
            channel: ch.channel,
            severity: ch.effective_severity(),
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
        self.channels
            .values()
            .filter(|c| c.allowed)
            .map(|c| c.effective_severity())
            .max()
            .unwrap_or(Severity::None)
    }

    /// "standby" | "advisory" | "alert"
    pub fn mode(&self) -> &'static str {
        if self.channels.values().any(|c| c.is_fullscreen()) {
            "alert"
        } else if self.channels.values().any(|c| c.allowed && c.effective_severity() >= Severity::Advisory) {
            "advisory"
        } else {
            "standby"
        }
    }

    /// Fullscreen-eligible telegrams (警報級以上の表示対象), by legacy display
    /// priority (国民保護＞緊急地震速報＞津波＞火山…), then severity, then most recent.
    pub fn alerts(&self) -> Vec<&AlertChannel> {
        let mut v: Vec<&AlertChannel> = self.channels.values().filter(|c| c.is_fullscreen()).collect();
        v.sort_by(|a, b| {
            b.category
                .priority()
                .cmp(&a.category.priority())
                .then(b.effective_severity().cmp(&a.effective_severity()))
                .then(b.rx_time_ms.cmp(&a.rx_time_ms))
        });
        v
    }

    /// Banner-level telegrams (表示対象だが全画面化しないもの), newest first.
    pub fn advisories(&self) -> Vec<&AlertChannel> {
        let mut v: Vec<&AlertChannel> = self
            .channels
            .values()
            .filter(|c| c.allowed && !c.is_fullscreen() && c.effective_severity() >= Severity::Advisory)
            .collect();
        v.sort_by(|a, b| b.rx_time_ms.cmp(&a.rx_time_ms));
        v
    }

    pub fn primary(&self) -> Option<&AlertChannel> {
        self.alerts().into_iter().next()
    }

    /// Which standby category lamps are currently lit.
    pub fn active_lamps(&self) -> Vec<LampGroup> {
        let mut v: Vec<LampGroup> = Vec::new();
        for c in self.channels.values().filter(|c| c.allowed && c.effective_severity() >= Severity::Advisory) {
            if let Some(g) = c.category.lamp_group() {
                if !v.contains(&g) {
                    v.push(g);
                }
            }
        }
        v
    }

    /// Mailbox history (緊急情報一覧), newest first.
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

    /// Per-telegram-type received counts, ordered like [`default_rules`].
    pub fn type_counts(&self) -> Vec<(AlertType, u64)> {
        [
            AlertType::Jalt,
            AlertType::Ifda,
            AlertType::Eprq,
            AlertType::Ioeq,
            AlertType::Issw,
            AlertType::Volc,
            AlertType::Wrma,
            AlertType::Unknown,
        ]
        .into_iter()
        .map(|t| (t, self.type_counts.get(t.code()).copied().unwrap_or(0)))
        .collect()
    }

    /// Inject a manually-raised alert (管理画面の手動発報). Builds a synthetic
    /// channel for the chosen 情報種別 + 地域 and ingests it like a received one.
    pub fn inject_manual(
        &mut self,
        alert_type: AlertType,
        category: Category,
        area: String,
        sub_type: String,
        headline: String,
    ) {
        let severity = if category == Category::Weather { Severity::Warning } else { Severity::None };
        let mut ch = AlertChannel {
            alert_type,
            category,
            allowed: true,
            head_title: area.clone(),
            area_name: area.clone(),
            sub_type,
            info_type: "発表".into(),
            headline,
            rx_time_ms: Utc::now().timestamp_millis(),
            severity,
            ..Default::default()
        };
        ch.key = format!("{}|{}", category.label(), area);
        self.ingest(ch);
    }

    /// Clear all live alert channels (手動発報の全解除). History is kept.
    pub fn clear_alerts(&mut self) {
        self.channels.clear();
    }

    /// Run a (simulated) 接続テスト. There is no real WAN/satellite uplink in
    /// this port, so it reports the local SDR# feed status instead.
    pub fn run_connect_test(&mut self) {
        let ok = self.receiver.connected;
        let message = if ok {
            format!("受信元 {} と接続できています（SDR# プラグイン）。", self.receiver.source)
        } else {
            format!("受信元 {} へ接続できません。プラグインの起動を確認してください。", self.receiver.source)
        };
        self.connect_test = Some(ConnectTestResult {
            at_ms: Utc::now().timestamp_millis(),
            ok,
            message,
        });
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
            "decoded": true, "rx_time_ms": 1i64, "chunk_type": "WRMA",
            "head_title": format!("{area}気象警報・注意報"),
            "info_type": status, "headline": "x", "xml": xml,
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
    fn disabling_display_rule_keeps_it_off_the_kiosk() {
        let mut st = AppState::new("test".into());
        // Turn off weather display.
        for r in &mut st.rules {
            if r.category == Category::Weather {
                r.display = false;
            }
        }
        st.ingest(from_json_line(&line("東京都", "大雨警報", "発表")).unwrap());
        assert_eq!(st.mode(), "standby"); // logged but not shown
        assert_eq!(st.inbox().count(), 1);
    }

    #[test]
    fn eew_takes_over_screen() {
        let mut st = AppState::new("test".into());
        let l = serde_json::json!({
            "decoded": true, "rx_time_ms": 9i64, "alert_type": "EPRQ",
            "alert_sub_type": "緊急地震速報", "info_type": "発表", "headline": "警戒",
        })
        .to_string();
        st.ingest(from_json_line(&l).unwrap());
        assert_eq!(st.mode(), "alert");
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
