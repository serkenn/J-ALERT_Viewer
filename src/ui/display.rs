//! Kiosk display: a calm standby screen, or a full-screen colour-coded alert for
//! 警報 / 特別警報 (注意報 appear only as a subdued banner).

use super::{hms, report_fmt, sev_color, sev_ink, App, StandbyStyle};
use chrono::Local;
use egui::{Align, Color32, FontId, Layout, RichText};
use jalert_receiver::model::Severity;

struct AlertView {
    severity: Severity,
    area: String,
    info_type: String,
    kinds: Vec<String>,
    headline: String,
    report_time: String,
    rx_time_ms: i64,
    sub_areas: Vec<String>,
    other_count: usize,
    others: Vec<String>,
}

impl App {
    pub(crate) fn show_display(&mut self, ctx: &egui::Context) {
        // Snapshot what we need, then release the lock before drawing.
        let (mode, primary, advisories) = {
            let st = self.state.lock().unwrap();
            let advisories: Vec<String> = st
                .advisories()
                .iter()
                .map(|c| format!("【{}】{}", area_of(&c.area_name, &c.head_title), c.kinds.iter().map(|k| k.name.clone()).collect::<Vec<_>>().join("・")))
                .collect();
            let alerts = st.alerts();
            let primary = alerts.first().map(|p| AlertView {
                severity: p.severity,
                area: area_of(&p.area_name, &p.head_title),
                info_type: p.info_type.clone(),
                kinds: p.kinds.iter().map(|k| k.name.clone()).collect(),
                headline: p.headline.clone(),
                report_time: p.report_time.clone(),
                rx_time_ms: p.rx_time_ms,
                sub_areas: p.areas.iter().filter(|a| **a != p.area_name).cloned().collect(),
                other_count: alerts.len().saturating_sub(1),
                others: alerts.iter().skip(1).map(|a| {
                    format!("{} {}：{}", a.severity.label(), area_of(&a.area_name, &a.head_title),
                            a.kinds.iter().map(|k| k.name.clone()).collect::<Vec<_>>().join("・"))
                }).collect(),
            });
            (st.mode(), primary, advisories)
        };

        match (mode, primary) {
            ("alert", Some(p)) => self.draw_alert(ctx, &p),
            _ => self.draw_standby(ctx, &advisories),
        }
    }

    fn draw_standby(&self, ctx: &egui::Context, advisories: &[String]) {
        match self.standby_style {
            StandbyStyle::Simple => self.draw_standby_simple(ctx, advisories),
            StandbyStyle::Jars2000 => self.draw_standby_jars(ctx, advisories),
        }
    }

    /// Calm clock screen (default).
    fn draw_standby_simple(&self, ctx: &egui::Context, advisories: &[String]) {
        let bg = Color32::from_rgb(0x0a, 0x10, 0x20);
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(bg))
            .show(ctx, |ui| {
                let h = ui.available_height();
                ui.vertical_centered(|ui| {
                    ui.add_space(h * 0.22);
                    ui.label(RichText::new("受 信 待 機 中").size(h * 0.035).color(Color32::from_rgb(0x7e, 0x8d, 0xb5)));
                    ui.add_space(h * 0.02);
                    let now = Local::now();
                    ui.label(RichText::new(now.format("%H:%M:%S").to_string()).font(FontId::proportional(h * 0.18)).color(Color32::from_rgb(0xe8, 0xee, 0xfc)));
                    ui.label(RichText::new(now.format("%Y年%-m月%-d日").to_string()).size(h * 0.045).color(Color32::from_rgb(0xaa, 0xb6, 0xdd)));
                    ui.add_space(h * 0.04);
                    ui.label(RichText::new("● 異常なし").size(h * 0.03).color(Color32::from_rgb(0x27, 0xd0, 0x7a)));
                });
                advisory_banner(ui, ui.max_rect(), advisories);
            });
    }

    /// J-ALERT (jars2000) homage: logo + categories + earth backdrop.
    fn draw_standby_jars(&self, ctx: &egui::Context, advisories: &[String]) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::BLACK))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                paint_space_backdrop(ui, rect, ctx.input(|i| i.time));
                let h = rect.height();
                let w = rect.width();
                let p = ui.painter();

                // --- J-ALERT wordmark ---
                let cx = rect.center().x;
                let logo_y = rect.top() + h * 0.16;
                p.text(
                    egui::pos2(cx, logo_y),
                    egui::Align2::CENTER_CENTER,
                    "J-ALERT",
                    FontId::proportional(h * 0.13),
                    Color32::from_rgb(0xff, 0x2a, 0x1f),
                );
                // subtle glow line under the wordmark
                let lw = w * 0.34;
                p.line_segment(
                    [egui::pos2(cx - lw, logo_y + h * 0.085), egui::pos2(cx + lw, logo_y + h * 0.085)],
                    egui::Stroke::new(2.0, Color32::from_rgb(0x8a, 0x12, 0x10)),
                );

                // --- category list (国民保護 / 地震 / 津波 / 火山) ---
                let cats = ["国民保護に関する情報", "地震情報", "津波情報", "火山情報"];
                let cat_size = (h * 0.05).min(w * 0.04);
                let left = rect.left() + w * 0.12;
                let mut y = rect.top() + h * 0.36;
                let step = cat_size * 1.9;
                for c in cats {
                    let sq = cat_size * 0.62;
                    let r = egui::Rect::from_min_size(egui::pos2(left, y - sq / 2.0), egui::vec2(sq, sq));
                    p.rect_filled(r, 1.0, Color32::from_rgb(0xe9, 0xee, 0xf6));
                    p.text(
                        egui::pos2(left + sq + cat_size * 0.55, y),
                        Align2_LEFT_CENTER(),
                        c,
                        FontId::proportional(cat_size),
                        Color32::from_rgb(0xe9, 0xee, 0xf6),
                    );
                    y += step;
                }

                // --- clock / status (top-right) ---
                let now = Local::now();
                p.text(
                    egui::pos2(rect.right() - w * 0.04, rect.top() + h * 0.06),
                    egui::Align2::RIGHT_CENTER,
                    now.format("%H:%M:%S").to_string(),
                    FontId::proportional(h * 0.06),
                    Color32::from_rgb(0xdf, 0xe7, 0xff),
                );
                p.text(
                    egui::pos2(rect.right() - w * 0.04, rect.top() + h * 0.115),
                    egui::Align2::RIGHT_CENTER,
                    now.format("%Y年%-m月%-d日").to_string(),
                    FontId::proportional(h * 0.03),
                    Color32::from_rgb(0x9d, 0xab, 0xcf),
                );

                // --- standby / health line (bottom-left) ---
                p.text(
                    egui::pos2(rect.left() + w * 0.04, rect.bottom() - h * 0.06),
                    Align2_LEFT_CENTER(),
                    "● 受信待機中 / 異常なし",
                    FontId::proportional(h * 0.028),
                    Color32::from_rgb(0x35, 0xd9, 0x86),
                );

                advisory_banner(ui, rect, advisories);
            });
    }

    fn draw_alert(&self, ctx: &egui::Context, p: &AlertView) {
        let bg = sev_color(p.severity);
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(bg).inner_margin(egui::Margin::same(0.0)))
            .show(ctx, |ui| {
                let full = ui.max_rect();
                let h = full.height();

                // Flashing overlay to draw the eye.
                let t = ctx.input(|i| i.time);
                let phase = (t * 1.1).fract();
                if phase > 0.55 && phase < 0.78 {
                    ui.painter().rect_filled(full, 0.0, Color32::from_white_alpha(28));
                }

                let ink = Color32::WHITE;
                ui.add_space(h * 0.04);
                ui.horizontal(|ui| {
                    ui.add_space(full.width() * 0.04);
                    ui.label(RichText::new(p.severity.label()).font(FontId::proportional(h * 0.12)).strong().color(ink));
                    ui.add_space(24.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&p.area).font(FontId::proportional(h * 0.075)).strong().color(ink));
                        let it = if p.info_type.is_empty() { "発表".to_string() } else { p.info_type.clone() };
                        let mut line = it;
                        if p.other_count > 0 {
                            line = format!("{line}　／　他 {} 件発表中", p.other_count);
                        }
                        ui.label(RichText::new(line).size(h * 0.032).color(ink));
                    });
                });

                ui.add_space(h * 0.04);
                // Kinds as chips.
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(full.width() * 0.04);
                    for k in &p.kinds {
                        let chip = RichText::new(k).font(FontId::proportional(h * 0.05)).strong().color(ink);
                        egui::Frame::none()
                            .fill(Color32::from_black_alpha(70))
                            .stroke(egui::Stroke::new(2.0, Color32::from_white_alpha(140)))
                            .rounding(10.0)
                            .inner_margin(egui::Margin::symmetric(16.0, 8.0))
                            .show(ui, |ui| { ui.label(chip); });
                        ui.add_space(10.0);
                    }
                });

                ui.add_space(h * 0.03);
                if !p.headline.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(full.width() * 0.04);
                        ui.add(egui::Label::new(RichText::new(&p.headline).size(h * 0.045).color(ink)).wrap());
                    });
                }
                if !p.sub_areas.is_empty() {
                    ui.add_space(h * 0.02);
                    ui.horizontal(|ui| {
                        ui.add_space(full.width() * 0.04);
                        ui.label(RichText::new(format!("対象地域　{}", p.sub_areas.join("　"))).size(h * 0.028).color(Color32::from_white_alpha(220)));
                    });
                }

                // Footer pinned to the bottom.
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.add_space(h * 0.03);
                    if !p.others.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.add_space(full.width() * 0.04);
                            for o in &p.others {
                                egui::Frame::none()
                                    .fill(Color32::from_black_alpha(60))
                                    .stroke(egui::Stroke::new(1.0, Color32::from_white_alpha(110)))
                                    .rounding(8.0)
                                    .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                                    .show(ui, |ui| { ui.label(RichText::new(o).size(h * 0.026).color(ink)); });
                                ui.add_space(8.0);
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(full.width() * 0.04);
                        ui.label(RichText::new(format!("発表時刻 {}", report_fmt(&p.report_time))).size(h * 0.026).color(Color32::from_white_alpha(230)));
                        ui.add_space(30.0);
                        ui.label(RichText::new(format!("受信 {}", hms(p.rx_time_ms))).size(h * 0.026).color(Color32::from_white_alpha(230)));
                    });
                });
                let _ = sev_ink; // referenced for parity with web tags
            });
    }
}

/// Yellow 注意報 banner pinned to the bottom of the standby screen.
fn advisory_banner(ui: &egui::Ui, rect: egui::Rect, advisories: &[String]) {
    if advisories.is_empty() {
        return;
    }
    let bar_h = (rect.height() * 0.08).max(40.0);
    let bar = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - bar_h), rect.max);
    let p = ui.painter();
    p.rect_filled(bar, 0.0, sev_color(Severity::Advisory));
    p.text(
        egui::pos2(bar.left() + 24.0, bar.center().y),
        Align2_LEFT_CENTER(),
        format!("注意報　{}", advisories.join("　／　")),
        FontId::proportional((bar_h * 0.4).min(26.0)),
        Color32::from_rgb(0x1a, 0x1a, 0x1c),
    );
}

/// A simple starfield with an earth limb glowing along the bottom — an homage to
/// the classic J-ALERT (jars2000) standby screen, drawn procedurally (no assets).
fn paint_space_backdrop(ui: &egui::Ui, rect: egui::Rect, time: f64) {
    let p = ui.painter();
    let w = rect.width();
    let h = rect.height();

    // deterministic star positions (seeded LCG) so they don't jump each frame
    let mut seed: u64 = 0x1234_5678_9abc_def1;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((seed >> 33) as f32) / (i32::MAX as f32)
    };
    for i in 0..130 {
        let sx = rect.left() + next() * w;
        let sy = rect.top() + next() * h * 0.72;
        let base = 0.35 + next() * 0.65;
        let tw = 0.75 + 0.25 * ((time * 1.3 + i as f64).sin() as f32);
        let a = (base * tw * 200.0) as u8;
        let r = 0.5 + next() * 1.3;
        p.circle_filled(egui::pos2(sx, sy), r, Color32::from_white_alpha(a));
    }

    // earth limb: a big circle whose top arc rises from the bottom edge
    let cx = rect.center().x;
    let radius = w * 0.62;
    let center = egui::pos2(cx, rect.bottom() + radius * 0.62);
    p.circle_filled(center, radius, Color32::from_rgb(0x07, 0x1a, 0x33));
    // landmass / lighter ocean hints
    p.circle_filled(center + egui::vec2(-w * 0.12, -radius * 0.18), radius * 0.34, Color32::from_rgb(0x0e, 0x33, 0x55));
    p.circle_filled(center + egui::vec2(w * 0.16, -radius * 0.12), radius * 0.22, Color32::from_rgb(0x10, 0x3a, 0x42));
    // atmosphere glow rim
    p.circle_stroke(center, radius + 1.0, egui::Stroke::new(3.0, Color32::from_rgb(0x36, 0x8f, 0xe0)));
    p.circle_stroke(center, radius + 6.0, egui::Stroke::new(8.0, Color32::from_rgba_unmultiplied(0x36, 0x8f, 0xe0, 40)));
    let _ = h;
}

fn area_of(area_name: &str, head_title: &str) -> String {
    if !area_name.is_empty() {
        area_name.to_string()
    } else {
        head_title.to_string()
    }
}

#[allow(non_snake_case)]
fn Align2_LEFT_CENTER() -> egui::Align2 {
    egui::Align2([egui::Align::Min, egui::Align::Center])
}
