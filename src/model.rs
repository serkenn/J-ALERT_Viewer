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
    Jalt, // 国民保護情報 (protect_civilians)
    Ifda, // 緊急連絡 (fire_department)
    Eprq, // 緊急地震速報 (emergency_earthquake)
    Ioeq, // 地震情報・震度速報 (earthquake)
    Issw, // 津波情報 (tsunami)
    Volc, // 火山情報 (volcano)
    Wrma, // 気象情報 (meteorological)
    #[default]
    Unknown,
}

impl AlertType {
    /// The 4-char code (`alert_type` symbol, upper-cased) as it appears in the
    /// telegram / logs.
    pub fn code(self) -> &'static str {
        match self {
            AlertType::Jalt => "JALT",
            AlertType::Ifda => "IFDA",
            AlertType::Eprq => "EPRQ",
            AlertType::Ioeq => "IOEQ",
            AlertType::Issw => "ISSW",
            AlertType::Volc => "VOLC",
            AlertType::Wrma => "WRMA",
            AlertType::Unknown => "----",
        }
    }

    pub fn from_code(s: &str) -> AlertType {
        match s.trim().to_ascii_uppercase().as_str() {
            "JALT" => AlertType::Jalt,
            "IFDA" => AlertType::Ifda,
            "EPRQ" => AlertType::Eprq,
            "IOEQ" => AlertType::Ioeq,
            "ISSW" => AlertType::Issw,
            "VOLC" => AlertType::Volc,
            "WRMA" | "WRMX" => AlertType::Wrma,
            _ => AlertType::Unknown,
        }
    }
}

/// 情報種別. Mirrors the legacy telegram taxonomy (`alert_type_text` /
/// `alert_sub_type`). Drives the colour, the standby category lamps, the display
/// priority and whether a telegram takes over the whole screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Category {
    CivilProtection,  // 国民保護情報 (jalt)
    EmergencyContact, // 緊急連絡 (ifda)
    Eew,              // 緊急地震速報 (eprq)
    Earthquake,       // 地震情報 (ioeq)
    SeismicIntensity, // 震度速報 (ioeq)
    Tsunami,          // 津波情報 (issw)
    Volcano,          // 火山情報 (volc)
    Weather,          // 気象情報 (wrma)
    Test,             // 試験放送 (jalt pattern 6 / 配信試験)
    #[default]
    Other, // その他
}

impl Category {
    /// 情報種別名, matching the legacy `alert_type_text`.
    pub fn label(self) -> &'static str {
        match self {
            Category::CivilProtection => "国民保護情報",
            Category::EmergencyContact => "緊急連絡",
            Category::Eew => "緊急地震速報",
            Category::Earthquake => "地震情報",
            Category::SeismicIntensity => "震度速報",
            Category::Tsunami => "津波情報",
            Category::Volcano => "火山情報",
            Category::Weather => "気象情報",
            Category::Test => "試験放送",
            Category::Other => "その他",
        }
    }

    /// Display priority, mirroring `Telegram#priority` (higher = more urgent):
    /// 国民保護 15 / 緊急地震速報 13 / 津波 11 / 火山 9 / 緊急連絡 5 /
    /// 震度速報 1 / 地震・気象 0.
    pub fn priority(self) -> u8 {
        match self {
            Category::CivilProtection => 15,
            Category::Eew => 13,
            Category::Tsunami => 11,
            Category::Volcano => 9,
            Category::EmergencyContact => 5,
            Category::Test => 2,
            Category::SeismicIntensity => 1,
            Category::Earthquake | Category::Weather | Category::Other => 0,
        }
    }

    /// Map a legacy `alert_sub_type` string to a category.
    pub fn from_sub_type(s: &str) -> Category {
        let s = s.trim();
        if s.contains("国民保護") || s.contains("弾道ミサイル") || s.contains("武力攻撃")
            || s.contains("ゲリラ") || s.contains("航空攻撃") || s.contains("テロ")
        {
            Category::CivilProtection
        } else if s.contains("緊急連絡") {
            Category::EmergencyContact
        } else if s.contains("緊急地震速報") {
            Category::Eew
        } else if s.contains("震度速報") {
            Category::SeismicIntensity
        } else if s.contains("地震") || s.contains("震源") || s.contains("震度") {
            // 「震源・震度情報」等（"地震" の文字を含まない見出し）も地震情報として扱う。
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

    /// The default category for a bare telegram code.
    pub fn from_alert_type(t: AlertType) -> Category {
        match t {
            AlertType::Jalt => Category::CivilProtection,
            AlertType::Ifda => Category::EmergencyContact,
            AlertType::Eprq => Category::Eew,
            AlertType::Ioeq => Category::Earthquake,
            AlertType::Issw => Category::Tsunami,
            AlertType::Volc => Category::Volcano,
            AlertType::Wrma => Category::Weather,
            AlertType::Unknown => Category::Other,
        }
    }

    /// The four standby "category lamps" the legacy receiver shows on its idle
    /// screen (国民保護 / 地震 / 津波 / 火山).
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
    pub sub_type: String,  // raw alert_sub_type text (used to pick the exact screen)
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
            Category::EmergencyContact => Severity::Warning,
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
