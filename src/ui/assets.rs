//! Bundled screen backgrounds, decoded to egui textures at startup.
//!
//! The standby image and the per-category full-screen alert backgrounds are the
//! genuine artwork recovered from the legacy receiver; they drive the「リアル」
//! standby/alert styles. The finer set (tsunami grades, volcano levels, civil-
//! protection variants) mirrors the legacy `TextBuilder#style` selection.
//! Files are stored under neutral names.

use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use jalert_receiver::model::Category;

macro_rules! img {
    ($name:literal) => {
        include_bytes!(concat!("../../assets/screen/", $name))
    };
}

const STANDBY: &[u8] = img!("standby.jpg");
// civil protection
const PROTECT: &[u8] = img!("cat_protect.gif"); // 国民保護(基本)
const PROTECT_MISSILE: &[u8] = img!("protect_missile.gif"); // 弾道ミサイル
// earthquake
const EQ_EEW: &[u8] = img!("cat_eq.gif"); // 緊急地震速報
const EQ_INFO: &[u8] = img!("eq_info.gif"); // 地震情報・震度速報
// tsunami
const TS_GREAT: &[u8] = img!("ts_great.gif"); // 大津波警報
const TS_WARNING: &[u8] = img!("cat_tsunami.gif"); // 津波警報
const TS_ADVISORY: &[u8] = img!("ts_advisory.gif"); // 津波注意報
const TS_INFO: &[u8] = img!("ts_info.gif"); // 津波情報/解除
// volcano
const VO_RED: &[u8] = img!("cat_volcano.gif"); // 噴火警報 Lv4-5
const VO_YELLOW: &[u8] = img!("vo_yellow.gif"); // Lv3
const VO_GRAY: &[u8] = img!("vo_gray.gif"); // Lv2
const VO_FORECAST: &[u8] = img!("vo_forecast.gif"); // Lv1/予報
const VO_SOKUHOU: &[u8] = img!("vo_sokuhou.gif"); // 噴火速報

/// A chosen full-screen alert background plus the heading the legacy receiver
/// shows for it (e.g. 大津波警報).
pub struct AlertScreen<'a> {
    pub tex: &'a TextureHandle,
    pub heading: &'static str,
}

/// Decoded screen textures. Each may be `None` if decoding failed, in which case
/// the caller falls back to a procedural / colour-coded rendering.
pub struct Screens {
    pub standby: Option<TextureHandle>,
    protect: Option<TextureHandle>,
    protect_missile: Option<TextureHandle>,
    eq_eew: Option<TextureHandle>,
    eq_info: Option<TextureHandle>,
    ts_great: Option<TextureHandle>,
    ts_warning: Option<TextureHandle>,
    ts_advisory: Option<TextureHandle>,
    ts_info: Option<TextureHandle>,
    vo_red: Option<TextureHandle>,
    vo_yellow: Option<TextureHandle>,
    vo_gray: Option<TextureHandle>,
    vo_forecast: Option<TextureHandle>,
    vo_sokuhou: Option<TextureHandle>,
}

fn tex(ctx: &Context, name: &str, bytes: &[u8]) -> Option<TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let color = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
    Some(ctx.load_texture(name, color, TextureOptions::LINEAR))
}

impl Screens {
    pub fn load(ctx: &Context) -> Self {
        Screens {
            standby: tex(ctx, "standby", STANDBY),
            protect: tex(ctx, "protect", PROTECT),
            protect_missile: tex(ctx, "protect_missile", PROTECT_MISSILE),
            eq_eew: tex(ctx, "eq_eew", EQ_EEW),
            eq_info: tex(ctx, "eq_info", EQ_INFO),
            ts_great: tex(ctx, "ts_great", TS_GREAT),
            ts_warning: tex(ctx, "ts_warning", TS_WARNING),
            ts_advisory: tex(ctx, "ts_advisory", TS_ADVISORY),
            ts_info: tex(ctx, "ts_info", TS_INFO),
            vo_red: tex(ctx, "vo_red", VO_RED),
            vo_yellow: tex(ctx, "vo_yellow", VO_YELLOW),
            vo_gray: tex(ctx, "vo_gray", VO_GRAY),
            vo_forecast: tex(ctx, "vo_forecast", VO_FORECAST),
            vo_sokuhou: tex(ctx, "vo_sokuhou", VO_SOKUHOU),
        }
    }

    /// Pick the genuine full-screen background + heading for an alert, mirroring
    /// the legacy `TextBuilder#style` rules. `hint` should carry whatever text we
    /// have (sub-type / headline / title / kinds) so grades & levels can be
    /// resolved. Returns `None` for categories with no artwork (→ colour fallback).
    pub fn alert_screen(&self, cat: Category, hint: &str) -> Option<AlertScreen<'_>> {
        let s = hint;
        let (opt, heading): (&Option<TextureHandle>, &'static str) = match cat {
            Category::CivilProtection => {
                if s.contains("弾道ミサイル") {
                    (&self.protect_missile, "弾道ミサイルに関する情報")
                } else {
                    (&self.protect, "国民保護に関する情報")
                }
            }
            Category::Eew => (&self.eq_eew, "緊急地震速報"),
            Category::Earthquake => (&self.eq_info, "地震情報"),
            Category::SeismicIntensity => (&self.eq_info, "震度速報"),
            Category::Tsunami => {
                if s.contains("大津波") {
                    (&self.ts_great, "大津波警報")
                } else if s.contains("津波警報") {
                    (&self.ts_warning, "津波警報")
                } else if s.contains("津波注意報") {
                    (&self.ts_advisory, "津波注意報")
                } else {
                    (&self.ts_info, "津波情報")
                }
            }
            Category::Volcano => {
                if s.contains("噴火速報") {
                    (&self.vo_sokuhou, "噴火速報")
                } else if s.contains("レベル５") || s.contains("レベル5") || s.contains("レベル４") || s.contains("レベル4") {
                    (&self.vo_red, "噴火警報")
                } else if s.contains("レベル３") || s.contains("レベル3") {
                    (&self.vo_yellow, "噴火警報")
                } else if s.contains("レベル２") || s.contains("レベル2") {
                    (&self.vo_gray, "噴火警報")
                } else if s.contains("レベル１") || s.contains("レベル1") || s.contains("予報") {
                    (&self.vo_forecast, "噴火予報")
                } else if s.contains("噴火警報") || s.contains("警報") {
                    (&self.vo_red, "噴火警報")
                } else {
                    (&self.vo_forecast, "火山情報")
                }
            }
            _ => return None,
        };
        opt.as_ref().map(|tex| AlertScreen { tex, heading })
    }
}
