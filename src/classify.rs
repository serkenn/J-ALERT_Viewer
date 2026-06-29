//! Turn one JSONL line from the SDR# plugin into an [`AlertChannel`], using the
//! legacy telegram model.
//!
//! Every line carries (at least) a telegram type code. We map it to an
//! [`AlertType`] and a [`Category`] (情報種別). Weather telegrams (WRMA) also
//! carry the inflated JMA XML, from which we grade severity authoritatively: the
//! Body/Warning block for the prefectural forecast area lists each Kind with a
//! Status of 発表 / 継続 / 解除; a Kind is "in force" when its Status is not
//! 解除/なし and its severity comes from the Name suffix (特別警報 > 警報 > 注意報).
//!
//! Non-weather categories (地震・津波・緊急地震速報・国民保護) don't need the XML;
//! their level is fixed by the category (see [`AlertChannel::effective_severity`]).

use crate::model::{AlertChannel, AlertKind, AlertType, Category, RxChannel, Severity};
use std::collections::BTreeSet;

/// Returns `None` for lines that carry no usable telegram.
pub fn from_json_line(line: &str) -> Option<AlertChannel> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(line).ok()?;

    // Only decoded telegrams are classifiable.
    if v.get("decoded").and_then(|b| b.as_bool()) != Some(true) {
        return None;
    }

    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let xml = s("xml");

    // Telegram type code: explicit `alert_type`, else the plugin's `chunk_type`.
    let type_code = {
        let at = s("alert_type");
        if !at.is_empty() {
            at
        } else {
            s("chunk_type")
        }
    };
    let alert_type = AlertType::from_code(&type_code);

    // 情報種別: explicit `alert_sub_type`, else inferred from the type / title.
    let category = {
        let sub = s("alert_sub_type");
        if !sub.is_empty() {
            Category::from_sub_type(&sub)
        } else if alert_type == AlertType::Wrma {
            Category::Weather
        } else {
            Category::from_alert_type(alert_type)
        }
    };

    // A line is usable if it has XML (weather) or any telegram typing.
    if xml.is_empty() && alert_type == AlertType::Unknown {
        return None;
    }

    let mut ch = AlertChannel {
        alert_type,
        category,
        allowed: true, // refined by the display settings in AppState::ingest
        title: s("title"),
        head_title: s("head_title"),
        sub_type: s("alert_sub_type"),
        info_type: s("info_type"),
        headline: s("headline"),
        report_time: s("report_time"),
        packet_time: s("packet_time"),
        rx_time_ms: v.get("rx_time_ms").and_then(|n| n.as_i64()).unwrap_or(0),
        channel: RxChannel::from_str(&{
            let c = s("channel");
            if c.is_empty() { s("source") } else { c }
        }),
        xml: xml.clone(),
        ..Default::default()
    };

    // Enrich from the JMA XML: fill any fields the JSON wrapper left empty
    // (real telegrams carry Control/Head Title, ReportDateTime, Headline/Text…)
    // and grade weather warnings.
    if !xml.is_empty() {
        if let Ok(doc) = roxmltree::Document::parse(&xml) {
            apply_head_fields(&doc, &mut ch);
            if ch.category == Category::Weather {
                classify_weather_doc(&doc, &mut ch);
            } else if matches!(ch.category, Category::Earthquake | Category::SeismicIntensity) {
                apply_quake_fields(&doc, &mut ch);
            }
        }
    }

    // De-duplication key: category + the prefectural area (or title).
    let area = if !ch.head_title.is_empty() {
        ch.head_title.clone()
    } else if !ch.title.is_empty() {
        ch.title.clone()
    } else {
        ch.packet_time.clone()
    };
    ch.key = format!("{}|{}", ch.category.label(), area);

    Some(ch)
}

fn classify_weather_doc(doc: &roxmltree::Document, ch: &mut AlertChannel) {
    // Prefer the prefectural-area Warning block; fall back to the first one.
    let warning = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "Warning")
        .find(|n| n.attribute("type").map_or(false, |t| t.contains("府県予報区")))
        .or_else(|| doc.descendants().find(|n| n.tag_name().name() == "Warning"));
    let warning = match warning {
        Some(w) => w,
        None => return,
    };

    let mut kinds: Vec<AlertKind> = Vec::new();
    let mut areas: BTreeSet<String> = BTreeSet::new();
    let mut top = Severity::None;

    for item in warning.children().filter(|n| n.tag_name().name() == "Item") {
        for kind in item.children().filter(|n| n.tag_name().name() == "Kind") {
            let name = child_text(kind, "Name");
            let status = child_text(kind, "Status");
            if name.is_empty() || status == "解除" || status == "なし" {
                continue;
            }
            let sev = Severity::of_name(&name);
            if sev == Severity::None {
                continue;
            }
            if sev > top {
                top = sev;
            }
            kinds.push(AlertKind { name, status, severity: sev });
        }
        if let Some(area) = item
            .children()
            .find(|n| n.tag_name().name() == "Area")
            .map(|a| child_text(a, "Name"))
        {
            if !area.is_empty() {
                areas.insert(area);
            }
        }
    }

    // Collapse duplicate kinds (same name across several area blocks).
    kinds.sort_by(|a, b| b.severity.cmp(&a.severity));
    let mut seen = BTreeSet::new();
    kinds.retain(|k| seen.insert(k.name.clone()));

    ch.kinds = kinds;
    ch.severity = top;
    ch.areas = areas.into_iter().collect();
    if ch.area_name.is_empty() {
        ch.area_name = ch.areas.first().cloned().unwrap_or_default();
    }
}

/// Build an [`AlertChannel`] straight from a raw JMA XML telegram (no JSON
/// wrapper) — used to replay the saved `.xml` archive. `packet_time` is the
/// 17-digit timestamp from the filename (or empty).
pub fn from_xml(xml: &str, packet_time: &str) -> Option<AlertChannel> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    let control = first_text(&doc, "Title"); // Control/Title (feed level)
    let info_kind = first_text(&doc, "InfoKind");
    let category = detect_category(&control, &info_kind);
    let alert_type = alert_type_for(category);

    let mut ch = AlertChannel {
        alert_type,
        category,
        allowed: true,
        sub_type: if info_kind.is_empty() { control.clone() } else { info_kind },
        packet_time: packet_time.to_string(),
        rx_time_ms: packet_time_to_ms(packet_time),
        channel: RxChannel::Satellite,
        xml: xml.to_string(),
        ..Default::default()
    };
    apply_head_fields(&doc, &mut ch);
    if category == Category::Weather {
        classify_weather_doc(&doc, &mut ch);
    } else if matches!(category, Category::Earthquake | Category::SeismicIntensity) {
        apply_quake_fields(&doc, &mut ch);
    }

    let area = if !ch.head_title.is_empty() {
        ch.head_title.clone()
    } else if !ch.title.is_empty() {
        ch.title.clone()
    } else {
        ch.packet_time.clone()
    };
    ch.key = format!("{}|{}", ch.category.label(), area);
    Some(ch)
}

/// Fill Control/Head titles, report time, info type and headline from the XML
/// where the channel doesn't already have them.
fn apply_head_fields(doc: &roxmltree::Document, ch: &mut AlertChannel) {
    if ch.title.is_empty() {
        ch.title = first_text(doc, "Title");
    }
    if ch.head_title.is_empty() {
        ch.head_title = head_title(doc);
    }
    if ch.report_time.is_empty() {
        ch.report_time = first_text(doc, "ReportDateTime");
    }
    if ch.info_type.is_empty() {
        ch.info_type = head_child_text(doc, "InfoType");
    }
    if ch.headline.is_empty() {
        ch.headline = headline_text(doc);
    }
}

/// Earthquake telegrams: surface hypocentre / max seismic intensity / magnitude.
fn apply_quake_fields(doc: &roxmltree::Document, ch: &mut AlertChannel) {
    let hypo = doc
        .descendants()
        .find(|n| n.tag_name().name() == "Hypocenter")
        .and_then(|h| h.descendants().find(|n| n.tag_name().name() == "Area"))
        .map(|a| child_text(a, "Name"))
        .unwrap_or_default();
    let max_int = first_text(doc, "MaxInt");
    let mag = doc
        .descendants()
        .find(|n| n.tag_name().name() == "Magnitude")
        .and_then(|n| n.text())
        .unwrap_or("")
        .trim()
        .to_string();

    if !hypo.is_empty() && ch.area_name.is_empty() {
        ch.area_name = hypo.clone();
    }
    let mut bits = Vec::new();
    if !max_int.is_empty() {
        bits.push(format!("最大震度 {max_int}"));
    }
    if !mag.is_empty() {
        bits.push(format!("M{mag}"));
    }
    if !hypo.is_empty() {
        bits.push(format!("震源 {hypo}"));
    }
    if !bits.is_empty() {
        let summary = bits.join("　");
        ch.headline = if ch.headline.is_empty() {
            summary
        } else {
            format!("{}　{}", ch.headline, summary)
        };
    }
}

fn detect_category(control_title: &str, info_kind: &str) -> Category {
    let s = format!("{control_title} {info_kind}");
    if s.contains("緊急地震速報") {
        Category::Eew
    } else if s.contains("震度速報") {
        Category::SeismicIntensity
    } else if s.contains("地震") || s.contains("震源") {
        Category::Earthquake
    } else if s.contains("津波") {
        Category::Tsunami
    } else if s.contains("噴火") || s.contains("火山") {
        Category::Volcano
    } else if s.contains("国民保護") {
        Category::CivilProtection
    } else if s.contains("気象") || s.contains("大雨") || s.contains("竜巻") || s.contains("土砂")
        || s.contains("洪水") || s.contains("警報") || s.contains("注意報")
    {
        Category::Weather
    } else {
        Category::Other
    }
}

fn alert_type_for(c: Category) -> AlertType {
    match c {
        Category::CivilProtection => AlertType::Jalt,
        Category::EmergencyContact => AlertType::Ifda,
        Category::Eew => AlertType::Eprq,
        Category::Earthquake | Category::SeismicIntensity => AlertType::Ioeq,
        Category::Tsunami => AlertType::Issw,
        Category::Volcano => AlertType::Volc,
        Category::Weather => AlertType::Wrma,
        Category::Test | Category::Other => AlertType::Unknown,
    }
}

/// First element in the document with the given local tag name; trimmed text.
fn first_text(doc: &roxmltree::Document, tag: &str) -> String {
    doc.descendants()
        .find(|n| n.tag_name().name() == tag)
        .and_then(|n| n.text())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// `<Head><Title>` (the prefecture-specific title), distinct from Control/Title.
fn head_title(doc: &roxmltree::Document) -> String {
    doc.descendants()
        .find(|n| n.tag_name().name() == "Head")
        .map(|h| child_text(h, "Title"))
        .unwrap_or_default()
}

fn head_child_text(doc: &roxmltree::Document, tag: &str) -> String {
    doc.descendants()
        .find(|n| n.tag_name().name() == "Head")
        .map(|h| child_text(h, tag))
        .unwrap_or_default()
}

/// `<Headline><Text>` — the human-readable summary sentence.
fn headline_text(doc: &roxmltree::Document) -> String {
    doc.descendants()
        .find(|n| n.tag_name().name() == "Headline")
        .map(|h| child_text(h, "Text"))
        .unwrap_or_default()
}

/// `YYYYMMDDhhmmssSSS` → epoch millis (local time); 0 if unparZable.
fn packet_time_to_ms(p: &str) -> i64 {
    use chrono::{Local, TimeZone};
    if p.len() < 14 {
        return 0;
    }
    let g = |a: usize, b: usize| p.get(a..b).and_then(|s| s.parse::<u32>().ok());
    match (g(0, 4), g(4, 6), g(6, 8), g(8, 10), g(10, 12), g(12, 14)) {
        (Some(y), Some(mo), Some(d), Some(h), Some(mi), Some(s)) => Local
            .with_ymd_and_hms(y as i32, mo, d, h, mi, s)
            .single()
            .map(|dt| dt.timestamp_millis())
            .unwrap_or(0),
        _ => 0,
    }
}

/// First direct child element with the given local name; trimmed text content.
fn child_text(node: roxmltree::Node, local: &str) -> String {
    node.children()
        .find(|n| n.tag_name().name() == local)
        .and_then(|n| n.text())
        .unwrap_or("")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_weather_xml_is_parsed() {
        let xml = "<Report xmlns=\"http://x/\">\
            <Control><Title>気象特別警報・警報・注意報</Title></Control>\
            <Head xmlns=\"http://x/b/\"><Title>東京都気象警報・注意報</Title>\
              <ReportDateTime>2026-06-28T19:46:00+09:00</ReportDateTime>\
              <InfoType>発表</InfoType>\
              <Headline><Text>東京都では、大雨に警戒してください。</Text></Headline></Head>\
            <Body xmlns=\"http://x/c/\">\
              <Warning type=\"気象警報・注意報（府県予報区等）\"><Item>\
                <Kind><Name>大雨警報</Name><Status>発表</Status></Kind>\
                <Area><Name>東京都</Name></Area></Item></Warning></Body></Report>";
        let c = from_xml(xml, "20260628194600000").unwrap();
        assert_eq!(c.alert_type, AlertType::Wrma);
        assert_eq!(c.category, Category::Weather);
        assert_eq!(c.head_title, "東京都気象警報・注意報");
        assert_eq!(c.title, "気象特別警報・警報・注意報");
        assert_eq!(c.severity, Severity::Warning);
        assert_eq!(c.area_name, "東京都");
        assert!(c.headline.contains("大雨に警戒"));
        assert!(c.report_time.starts_with("2026-06-28T19:46"));
        assert!(c.rx_time_ms > 0);
    }

    #[test]
    fn raw_quake_xml_extracts_intensity() {
        let xml = "<Report xmlns=\"http://x/\" xmlns:jmx_eb=\"http://e/\">\
            <Control><Title>震源・震度に関する情報</Title></Control>\
            <Head xmlns=\"http://x/b/\"><Title>震源・震度情報</Title>\
              <InfoKind>地震情報</InfoKind>\
              <Headline><Text>地震がありました。</Text></Headline></Head>\
            <Body xmlns=\"http://s/\">\
              <Earthquake><Hypocenter><Area><Name>茨城県沖</Name></Area></Hypocenter>\
                <jmx_eb:Magnitude>3.3</jmx_eb:Magnitude></Earthquake>\
              <Intensity><Observation><MaxInt>1</MaxInt></Observation></Intensity></Body></Report>";
        let c = from_xml(xml, "20260628233400000").unwrap();
        assert_eq!(c.category, Category::Earthquake);
        assert_eq!(c.alert_type, AlertType::Ioeq);
        assert_eq!(c.area_name, "茨城県沖");
        assert!(c.headline.contains("最大震度 1"));
        assert!(c.headline.contains("M3.3"));
    }

    fn jline(area: &str, kinds: &[(&str, &str)], info: &str) -> String {
        let items: String = kinds
            .iter()
            .map(|(n, st)| format!("<Kind><Name>{n}</Name><Code>03</Code><Status>{st}</Status></Kind>"))
            .collect();
        let xml = format!(
            "<?xml version=\"1.0\"?><Report xmlns=\"http://x/\">\
             <Head xmlns=\"http://x/b/\"><Title>{area}気象警報・注意報</Title></Head>\
             <Body xmlns=\"http://x/c/\">\
             <Warning type=\"気象警報・注意報（府県予報区等）\"><Item>{items}\
             <Area><Name>{area}</Name><Code>014030</Code></Area></Item></Warning></Body></Report>"
        );
        serde_json::json!({
            "decoded": true, "rx_time_ms": 1_000i64, "chunk_type": "WRMA",
            "head_title": format!("{area}気象警報・注意報"), "info_type": info,
            "headline": "テスト", "packet_time": "20260628200000000", "xml": xml,
        })
        .to_string()
    }

    #[test]
    fn weather_warning_is_classified() {
        let c = from_json_line(&jline("東京都", &[("大雨警報", "発表"), ("雷注意報", "発表")], "発表")).unwrap();
        assert_eq!(c.alert_type, AlertType::Wrma);
        assert_eq!(c.category, Category::Weather);
        assert_eq!(c.severity, Severity::Warning);
        assert_eq!(c.area_name, "東京都");
        assert_eq!(c.kinds.len(), 2);
        assert_eq!(c.kinds[0].name, "大雨警報"); // most severe first
        assert!(c.is_fullscreen());
    }

    #[test]
    fn emergency_beats_warning() {
        let c = from_json_line(&jline("沖縄", &[("大雨特別警報", "発表"), ("暴風警報", "発表")], "発表")).unwrap();
        assert_eq!(c.effective_severity(), Severity::Emergency);
    }

    #[test]
    fn cancelled_kinds_drop_severity() {
        let c = from_json_line(&jline("東京都", &[("大雨警報", "解除"), ("雷注意報", "継続")], "更新")).unwrap();
        assert_eq!(c.severity, Severity::Advisory); // only 雷注意報 remains in force
        assert!(!c.is_fullscreen());
    }

    #[test]
    fn eew_telegram_classifies_without_xml() {
        let line = serde_json::json!({
            "decoded": true, "rx_time_ms": 2i64, "alert_type": "EPRQ",
            "alert_sub_type": "緊急地震速報", "info_type": "発表",
            "headline": "強い揺れに警戒", "channel": "衛星系",
        })
        .to_string();
        let c = from_json_line(&line).unwrap();
        assert_eq!(c.alert_type, AlertType::Eprq);
        assert_eq!(c.category, Category::Eew);
        assert_eq!(c.channel, RxChannel::Satellite);
        assert!(c.is_fullscreen());
    }

    #[test]
    fn civil_protection_is_emergency() {
        let line = serde_json::json!({
            "decoded": true, "rx_time_ms": 3i64, "alert_type": "JALT",
            "alert_sub_type": "国民保護情報", "info_type": "発表",
            "headline": "ミサイル発射情報",
        })
        .to_string();
        let c = from_json_line(&line).unwrap();
        assert_eq!(c.category, Category::CivilProtection);
        assert_eq!(c.effective_severity(), Severity::Emergency);
    }

    #[test]
    fn seismic_intensity_is_advisory_banner() {
        let line = serde_json::json!({
            "decoded": true, "rx_time_ms": 4i64, "alert_type": "IOEQ",
            "alert_sub_type": "震度速報", "info_type": "発表",
        })
        .to_string();
        let c = from_json_line(&line).unwrap();
        assert_eq!(c.category, Category::SeismicIntensity);
        assert_eq!(c.effective_severity(), Severity::Advisory);
        assert!(!c.is_fullscreen());
    }

    #[test]
    fn non_decoded_is_ignored() {
        assert!(from_json_line(r#"{"decoded":false,"chunk_type":"abcd"}"#).is_none());
    }
}
