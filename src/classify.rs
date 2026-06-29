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

    if alert_type == AlertType::Wrma && !xml.is_empty() {
        classify_weather_xml(&xml, &mut ch);
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

fn classify_weather_xml(xml: &str, ch: &mut AlertChannel) {
    let doc = match roxmltree::Document::parse(xml) {
        Ok(d) => d,
        Err(_) => return,
    };

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
