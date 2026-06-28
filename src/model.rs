//! Core data types shared by the receiver, classifier, state store and UI.

/// Warning severity, ordered. Higher = more severe.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub enum Severity {
    #[default]
    None = 0,
    Advisory = 1,  // 注意報
    Warning = 2,   // 警報
    Emergency = 3, // 特別警報
}

impl Severity {
    /// Label as it appears in the mailbox / tags. `None` means a cancellation.
    pub fn label(self) -> &'static str {
        match self {
            Severity::Emergency => "特別警報",
            Severity::Warning => "警報",
            Severity::Advisory => "注意報",
            Severity::None => "解除",
        }
    }

    /// Severity implied by a warning name's suffix (特別警報 must be tested first).
    pub fn of_name(name: &str) -> Severity {
        if name.ends_with("特別警報") {
            Severity::Emergency
        } else if name.ends_with("警報") {
            Severity::Warning
        } else if name.ends_with("注意報") {
            Severity::Advisory
        } else {
            Severity::None
        }
    }
}

/// One warning kind in force for an area, e.g. 大雨警報 / 発表.
#[derive(Clone, Debug)]
pub struct AlertKind {
    pub name: String,
    pub status: String, // 発表 / 継続 / 解除
    pub severity: Severity,
}

/// The state of one warning "channel", keyed by the prefectural forecast area
/// (`head_title`). A newer report supersedes the previous one.
#[derive(Clone, Debug, Default)]
pub struct AlertChannel {
    pub key: String,
    pub title: String,
    pub head_title: String,
    pub area_name: String,
    pub info_type: String,
    pub headline: String,
    pub report_time: String,
    pub packet_time: String,
    pub rx_time_ms: i64,
    pub severity: Severity,
    pub kinds: Vec<AlertKind>,
    pub areas: Vec<String>,
    pub xml: String,
}

/// One received telegram as it appears in the mailbox (read/unread).
#[derive(Clone, Debug, Default)]
pub struct InboxItem {
    pub id: u64,
    pub rx_time_ms: i64,
    pub packet_time: String,
    pub severity: Severity,
    pub info_type: String,
    pub title: String,
    pub head_title: String,
    pub area_name: String,
    pub kinds: Vec<String>,
    pub headline: String,
    pub report_time: String,
    pub read: bool,
    pub xml: String,
}

/// Health of the upstream link to the SDR# plugin.
#[derive(Clone, Debug, Default)]
pub struct ReceiverStatus {
    pub connected: bool,
    pub source: String,
    pub last_line_ms: i64,
    pub total_lines: u64,
}
