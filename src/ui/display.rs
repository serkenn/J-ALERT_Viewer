//! Kiosk display: a calm standby screen, or a full-screen colour-coded alert for
//! 警報 / 特別警報 (注意報 appear only as a subdued banner).

use super::{hms, report_fmt, sev_color, sev_ink, App};
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

                if !advisories.is_empty() {
                    let rect = ui.max_rect();
                    let bar_h = (h * 0.08).max(40.0);
                    let bar = egui::Rect::from_min_max(
                        egui::pos2(rect.left(), rect.bottom() - bar_h),
                        rect.max,
                    );
                    ui.painter().rect_filled(bar, 0.0, sev_color(Severity::Advisory));
                    let text = format!("注意報　{}", advisories.join("　／　"));
                    ui.painter().text(
                        egui::pos2(bar.left() + 24.0, bar.center().y),
                        Align2_LEFT_CENTER(),
                        text,
                        FontId::proportional((bar_h * 0.4).min(26.0)),
                        Color32::from_rgb(0x1a, 0x1a, 0x1c),
                    );
                }
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
