//! Core data types shared by the receiver, classifier, state store and UI.
//!
//! Modeled on the legacy receiver's telegram structure: every received packet
//! carries an `alert_type` (a 4-char telegram code such as WRMA / IOEQ / EPRQ /
//! ISSW / JALT), an `alert_sub_type` (情報種別 — what kind of information it is)
//! and an `allowed` flag (whether the 緊急情報表示設定 lets it reach the
//! display). We mirror those here rather than the old weather-only model.

/// 電文種別コード (`alert_type`) as broadcast on the alert channels.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AlertType {
    Wrma, // 気象警報・注意報
    Ioeq, // 地震情報・震度速報
    Eprq, // 緊急地震速報
    Issw, // 津波警報・注意報
    Jalt, // 試験・訓練・システム通知
    #[default]
    Unknown,
}

impl AlertType {
    /// The 4-char code as it appears in the telegram / logs.
    pub fn code(self) -> &'static str {
        match self {
            AlertType::Wrma => "WRMA",
            AlertType::Ioeq => "IOEQ",
            AlertType::Eprq => "EPRQ",
            AlertType::Issw => "ISSW",
            AlertType::Jalt => "JALT",
            AlertType::Unknown => "----",
        }
    }

    pub fn from_code(s: &str) -> AlertType {
        match s.trim().to_ascii_uppercase().as_str() {
            "WRMA" | "WRMX" => AlertType::Wrma,
            "IOEQ" => AlertType::Ioeq,
            "EPRQ" => AlertType::Eprq,
            "ISSW" => AlertType::Issw,
            "JALT" => AlertType::Jalt,
            _ => AlertType::Unknown,
        }
    }
}

/// 情報種別 (`alert_sub_type`). Drives the colour, the standby category lamps and
/// whether a telegram takes over the whole screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Category {
    CivilProtection,  // 国民保護情報 (弾道ミサイル・武力攻撃等)
    Eew,              // 緊急地震速報
    Earthquake,       // 地震情報
    SeismicIntensity, // 震度速報
    Tsunami,          // 津波警報・注意報
    Volcano,          // 火山(噴火警報)
    Weather,          // 気象警報・注意報
    Test,             // 試験・訓練
    #[default]
    Other, // その他
}

impl Category {
    /// 情報種別名 (long form), matching the legacy `alert_sub_type` wording.
    pub fn label(self) -> &'static str {
        match self {
            Category::CivilProtection => "国民保護情報",
            Category::Eew => "緊急地震速報",
            Category::Earthquake => "地震情報",
            Category::SeismicIntensity => "震度速報",
            Category::Tsunami => "津波警報・注意報",
            Category::Volcano => "火山情報",
            Category::Weather => "気象警報・注意報",
            Category::Test => "試験・訓練",
            Category::Other => "その他",
        }
    }

    /// Map a legacy `alert_sub_type` string to a category.
    pub fn from_sub_type(s: &str) -> Category {
        let s = s.trim();
        if s.contains("国民保護") || s.contains("弾道ミサイル") || s.contains("武力攻撃") {
            Category::CivilProtection
        } else if s.contains("緊急地震速報") {
            Category::Eew
        } else if s.contains("震度速報") {
            Category::SeismicIntensity
        } else if s.contains("地震") {
            Category::Earthquake
        } else if s.contains("津波") {
            Category::Tsunami
        } else if s.contains("噴火") || s.contains("火山") {
            Category::Volcano
        } else if s.contains("気象") {
            Category::Weather
        } else if s.contains("試験") || s.contains("訓練") || s.contains("テスト") {
            Category::Test
        } else {
            Category::Other
        }
    }

    /// The default category for a bare telegram code, used when no sub-type text
    /// is available.
    pub fn from_alert_type(t: AlertType) -> Category {
        match t {
            AlertType::Wrma => Category::Weather,
            AlertType::Ioeq => Category::Earthquake,
            AlertType::Eprq => Category::Eew,
            AlertType::Issw => Category::Tsunami,
            AlertType::Jalt => Category::Test,
            AlertType::Unknown => Category::Other,
        }
    }

    /// The four standby "category lamps" the legacy receiver shows on its idle screen.
    pub fn lamp_group(self) -> Option<LampGroup> {
        match self {
            Category::CivilProtection => Some(LampGroup::CivilProtection),
            Category::Eew | Category::Earthquake | Category::SeismicIntensity => {
                Some(LampGroup::Earthquake)
            }
            Category::Tsunami => Some(LampGroup::Tsunami),
            Category::Volcano => Some(LampGroup::Volcano),
            _ => None,
        }
    }
}

/// The four headline information groups shown as lamps on the standby screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LampGroup {
    CivilProtection, // 国民保護に関する情報
    Earthquake,      // 地震情報
    Tsunami,         // 津波情報
    Volcano,         // 火山情報
}

impl LampGroup {
    pub fn label(self) -> &'static str {
        match self {
            LampGroup::CivilProtection => "国民保護に関する情報",
            LampGroup::Earthquake => "地震情報",
            LampGroup::Tsunami => "津波情報",
            LampGroup::Volcano => "火山情報",
        }
    }
    pub const ALL: [LampGroup; 4] = [
        LampGroup::CivilProtection,
        LampGroup::Earthquake,
        LampGroup::Tsunami,
        LampGroup::Volcano,
    ];
}

/// The channel a packet arrived on. The legacy receiver receives the same telegram on both
/// the satellite and terrestrial paths and de-duplicates by CRC32.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RxChannel {
    Satellite,   // 衛星系チャネル
    Terrestrial, // 地上系チャネル
    #[default]
    Unknown,
}

impl RxChannel {
    pub fn label(self) -> &'static str {
        match self {
            RxChannel::Satellite => "衛星系",
            RxChannel::Terrestrial => "地上系",
            RxChannel::Unknown => "—",
        }
    }
    pub fn from_str(s: &str) -> RxChannel {
        let s = s.trim();
        if s.contains("衛星") || s.eq_ignore_ascii_case("satellite") || s.eq_ignore_ascii_case("sat") {
            RxChannel::Satellite
        } else if s.contains("地上") || s.eq_ignore_ascii_case("terrestrial") || s.eq_ignore_ascii_case("ground") {
            RxChannel::Terrestrial
        } else {
            RxChannel::Unknown
        }
    }
}

/// Warning severity, ordered. Higher = more severe. Used directly for weather and
/// as the unified display level for every category (see
/// [`AlertChannel::effective_severity`]).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub enum Severity {
    #[default]
    None = 0,
    Advisory = 1,  // 注意報 / 情報
    Warning = 2,   // 警報
    Emergency = 3, // 特別警報 / 国民保護
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

/// The state of one warning "channel", keyed by category + prefectural forecast
/// area. A newer report supersedes the previous one.
#[derive(Clone, Debug, Default)]
pub struct AlertChannel {
    pub key: String,
    pub alert_type: AlertType,
    pub category: Category,
    pub allowed: bool, // 緊急情報表示設定の表示対象か
    pub title: String,
    pub head_title: String,
    pub area_name: String,
    pub info_type: String, // 発表 / 継続 / 解除 / 訓練
    pub headline: String,
    pub report_time: String,
    pub packet_time: String,
    pub rx_time_ms: i64,
    pub channel: RxChannel,
    pub severity: Severity, // weather sub-level (特別警報/警報/注意報); else None
    pub kinds: Vec<AlertKind>,
    pub areas: Vec<String>,
    pub xml: String,
}

impl AlertChannel {
    /// True if `info_type` marks this report as an all-clear / cancellation.
    pub fn is_cancel(&self) -> bool {
        self.info_type.contains("解除") || self.info_type.contains("取消")
    }

    /// The unified display level across every category. This is what decides the
    /// colour, the standby↔fullscreen switch and the sort order. Weather keeps
    /// its own graded severity; other categories map to a fixed level.
    pub fn effective_severity(&self) -> Severity {
        if self.is_cancel() {
            return Severity::None;
        }
        match self.category {
            Category::CivilProtection => Severity::Emergency,
            Category::Eew | Category::Tsunami | Category::Volcano => Severity::Warning,
            Category::Weather => self.severity,
            Category::Earthquake | Category::SeismicIntensity => Severity::Advisory,
            Category::Test => Severity::Advisory,
            Category::Other => self.severity,
        }
    }

    /// The legacy receiver takes over the whole screen for 警報級以上 of an allowed telegram.
    pub fn is_fullscreen(&self) -> bool {
        self.allowed && self.effective_severity() >= Severity::Warning
    }

    /// Heading shown on the alert / lists.
    pub fn area_label(&self) -> &str {
        if !self.area_name.is_empty() {
            &self.area_name
        } else {
            &self.head_title
        }
    }
}

/// One received telegram as it appears in the 緊急情報一覧 (read/unread mailbox).
#[derive(Clone, Debug, Default)]
pub struct InboxItem {
    pub id: u64,
    pub rx_time_ms: i64,
    pub packet_time: String,
    pub alert_type: AlertType,
    pub category: Category,
    pub allowed: bool,
    pub channel: RxChannel,
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
