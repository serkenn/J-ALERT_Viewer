//! Kiosk display: a calm standby screen, or a full-screen colour-coded alert for
//! 警報級以上の表示対象 (注意報・情報 appear only as a subdued banner). Three
//! standby styles: シンプル / パチモン / リアル.

use super::{hms, report_fmt, sev_color, App, Align2_LEFT_CENTER, StandbyStyle};
use chrono::Local;
use egui::{Align, Color32, FontId, Layout, RichText};
use jalert_receiver::model::{Category, LampGroup, Severity};

struct AlertView {
    severity: Severity,
    category: Category,
    type_code: &'static str,
    area: String,
    sub_type: String,
    info_type: String,
    kinds: Vec<String>,
    headline: String,
    report_time: String,
    rx_time_ms: i64,
    sub_areas: Vec<String>,
    other_count: usize,
    others: Vec<String>,
}

impl AlertView {
    /// Text the screen selector matches against to pick the exact background.
    fn screen_hint(&self) -> String {
        format!("{} {} {}", self.sub_type, self.headline, self.kinds.join(" "))
    }
}

impl AlertView {
    /// The big heading: weather shows its graded level, other categories show
    /// the 情報種別 name.
    fn heading(&self) -> String {
        if self.category == Category::Weather {
            self.severity.label().to_string()
        } else {
            self.category.label().to_string()
        }
    }
}

impl App {
    pub(crate) fn show_display(&mut self, ctx: &egui::Context) {
        // Snapshot what we need, then release the lock before drawing.
        let (mode, primary, advisories, lamps) = {
            let st = self.state.lock().unwrap();
            let advisories: Vec<String> = st
                .advisories()
                .iter()
                .map(|c| {
                    let body = if c.kinds.is_empty() {
                        c.category.label().to_string()
                    } else {
                        c.kinds.iter().map(|k| k.name.clone()).collect::<Vec<_>>().join("・")
                    };
                    format!("【{}】{}", c.area_label(), body)
                })
                .collect();
            let alerts = st.alerts();
            let primary = alerts.first().map(|p| AlertView {
                severity: p.effective_severity(),
                category: p.category,
                type_code: p.alert_type.code(),
                area: p.area_label().to_string(),
                sub_type: p.sub_type.clone(),
                info_type: p.info_type.clone(),
                kinds: p.kinds.iter().map(|k| k.name.clone()).collect(),
                headline: p.headline.clone(),
                report_time: p.report_time.clone(),
                rx_time_ms: p.rx_time_ms,
                sub_areas: p.areas.iter().filter(|a| **a != p.area_name).cloned().collect(),
                other_count: alerts.len().saturating_sub(1),
                others: alerts.iter().skip(1).map(|a| {
                    let body = a.kinds.iter().map(|k| k.name.clone()).collect::<Vec<_>>().join("・");
                    let body = if body.is_empty() { a.category.label().to_string() } else { body };
                    format!("{} {}：{}", a.category.label(), a.area_label(), body)
                }).collect(),
            });
            (st.mode().to_string(), primary, advisories, st.active_lamps())
        };

        // Identity of the current primary alert (for sound rising-edge + ACK).
        let alert_key = primary
            .as_ref()
            .map(|p| format!("{}|{}|{}|{}", p.category.label(), p.area, p.severity.label(), p.info_type));

        // Rising-edge: a *new* full-screen alert fires the chime + announcement.
        if mode == "alert" {
            if let (Some(key), Some(p)) = (&alert_key, &primary) {
                if self.last_alert_key.as_deref() != Some(key.as_str()) {
                    self.play_alert_sound(p.category);
                    self.last_alert_key = Some(key.clone());
                }
            }
        } else {
            self.last_alert_key = None;
            self.acked = None; // nothing in force → reset acknowledgement
        }

        match (mode.as_str(), primary) {
            ("alert", Some(p)) => {
                let key = alert_key.unwrap_or_default();
                if self.acked.as_deref() == Some(key.as_str()) {
                    // Acknowledged: return to standby, but keep an "ongoing" ribbon.
                    self.draw_standby(ctx, &advisories, &lamps);
                    if ack_ribbon(ctx, &p) {
                        self.acked = None; // 再表示
                    }
                } else {
                    if self.standby_style == StandbyStyle::Real {
                        self.draw_alert_real(ctx, &p);
                    } else {
                        self.draw_alert(ctx, &p);
                    }
                    let pressed = ctx.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space));
                    if ack_button(ctx) || pressed {
                        self.acked = Some(key);
                    }
                }
            }
            _ => self.draw_standby(ctx, &advisories, &lamps),
        }
    }

    fn draw_standby(&self, ctx: &egui::Context, advisories: &[String], lamps: &[LampGroup]) {
        match self.standby_style {
            StandbyStyle::Simple => self.draw_standby_simple(ctx, advisories),
            StandbyStyle::Pachimon => self.draw_standby_pachimon(ctx, advisories, lamps),
            StandbyStyle::Real => self.draw_standby_real(ctx, advisories, lamps),
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

    /// パチモン: the stylised J-ALERT homage — logo + categories + earth backdrop.
    fn draw_standby_pachimon(&self, ctx: &egui::Context, advisories: &[String], lamps: &[LampGroup]) {
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
                let lw = w * 0.34;
                p.line_segment(
                    [egui::pos2(cx - lw, logo_y + h * 0.085), egui::pos2(cx + lw, logo_y + h * 0.085)],
                    egui::Stroke::new(2.0, Color32::from_rgb(0x8a, 0x12, 0x10)),
                );

                // --- category list with lamps ---
                let cat_size = (h * 0.05).min(w * 0.04);
                let left = rect.left() + w * 0.12;
                let mut y = rect.top() + h * 0.36;
                let step = cat_size * 1.9;
                for g in LampGroup::ALL {
                    let lit = lamps.contains(&g);
                    let sq = cat_size * 0.62;
                    let r = egui::Rect::from_min_size(egui::pos2(left, y - sq / 2.0), egui::vec2(sq, sq));
                    let lamp = if lit { Color32::from_rgb(0xff, 0x3a, 0x2a) } else { Color32::from_rgb(0xe9, 0xee, 0xf6) };
                    p.rect_filled(r, 1.0, lamp);
                    p.text(
                        egui::pos2(left + sq + cat_size * 0.55, y),
                        Align2_LEFT_CENTER(),
                        g.label(),
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

    /// リアル: the genuine standby screen — the real bundled image, aspect-fit on
    /// black, with the four information lamps lit red when a group is active. If
    /// the image failed to decode we fall back to the パチモン homage.
    fn draw_standby_real(&self, ctx: &egui::Context, advisories: &[String], lamps: &[LampGroup]) {
        let tex = match self.screens.standby.as_ref() {
            Some(t) => t,
            None => return self.draw_standby_pachimon(ctx, advisories, lamps),
        };
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::BLACK))
            .show(ctx, |ui| {
                let full = ui.max_rect();
                let img = fit_contain(full, tex.size_vec2());
                ui.painter().image(tex.id(), img, UV_FULL, Color32::WHITE);

                // The standby artwork has four white lamp squares down the left;
                // overlay red on the ones whose information group is active.
                let ys = [0.236_f32, 0.319, 0.401, 0.483];
                let sz = img.width() * 0.030;
                for (i, g) in LampGroup::ALL.iter().enumerate() {
                    if lamps.contains(g) {
                        let c = egui::pos2(img.left() + img.width() * 0.063, img.top() + img.height() * ys[i]);
                        ui.painter().rect_filled(
                            egui::Rect::from_center_size(c, egui::vec2(sz, sz)),
                            1.0,
                            Color32::from_rgb(0xff, 0x2a, 0x1f),
                        );
                    }
                }

                advisory_banner(ui, full, advisories);
            });
    }

    /// リアル: a full-screen alert using the genuine per-category artwork, with the
    /// dynamic text overlaid on a translucent panel. Weather (no artwork) falls
    /// back to the colour-coded screen.
    fn draw_alert_real(&self, ctx: &egui::Context, p: &AlertView) {
        let screen = match self.screens.alert_screen(p.category, &p.screen_hint()) {
            Some(s) => s,
            None => return self.draw_alert(ctx, p),
        };
        let tex = screen.tex;
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::BLACK).inner_margin(egui::Margin::same(0.0)))
            .show(ctx, |ui| {
                let full = ui.max_rect();
                let img = fit_contain(full, tex.size_vec2());
                ui.painter().image(tex.id(), img, UV_FULL, Color32::WHITE);

                // Flashing overlay to draw the eye.
                let phase = (ctx.input(|i| i.time) * 1.1).fract();
                if phase > 0.55 && phase < 0.78 {
                    ui.painter().rect_filled(full, 0.0, Color32::from_white_alpha(26));
                }

                let h = full.height();
                let panel = egui::Rect::from_min_max(
                    egui::pos2(img.left() + img.width() * 0.05, img.top() + img.height() * 0.44),
                    egui::pos2(img.right() - img.width() * 0.05, img.bottom() - img.height() * 0.05),
                );
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(panel), |ui| {
                    egui::Frame::none()
                        .fill(Color32::from_black_alpha(150))
                        .rounding(10.0)
                        .inner_margin(egui::Margin::symmetric(24.0, 18.0))
                        .show(ui, |ui| {
                            ui.set_width(panel.width());
                            let ink = Color32::WHITE;
                            ui.label(RichText::new(screen.heading).font(FontId::proportional(h * 0.06)).strong().color(ink));
                            ui.label(RichText::new(&p.area).font(FontId::proportional(h * 0.065)).strong().color(ink));
                            let mut line = if p.info_type.is_empty() { "発表".to_string() } else { p.info_type.clone() };
                            if p.other_count > 0 {
                                line = format!("{line}　／　他 {} 件発表中", p.other_count);
                            }
                            ui.label(RichText::new(format!("{}　{}", p.category.label(), line)).size(h * 0.03).color(ink));
                            if !p.kinds.is_empty() {
                                ui.add_space(h * 0.01);
                                ui.label(RichText::new(p.kinds.join("　")).size(h * 0.038).strong().color(ink));
                            }
                            if !p.headline.is_empty() {
                                ui.add_space(h * 0.01);
                                ui.add(egui::Label::new(RichText::new(&p.headline).size(h * 0.034).color(ink)).wrap());
                            }
                            ui.add_space(h * 0.012);
                            ui.label(
                                RichText::new(format!("発表時刻 {}　／　受信 {}", report_fmt(&p.report_time), hms(p.rx_time_ms)))
                                    .size(h * 0.024)
                                    .color(Color32::from_white_alpha(220)),
                            );
                        });
                });
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
                    ui.vertical(|ui| {
                        ui.label(RichText::new(p.heading()).font(FontId::proportional(h * 0.12)).strong().color(ink));
                        ui.label(RichText::new(format!("{}  電文種別 {}", p.category.label(), p.type_code)).size(h * 0.028).color(Color32::from_white_alpha(220)));
                    });
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
                // Kinds as chips (weather); other categories may have none.
                if !p.kinds.is_empty() {
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
                }

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
            });
    }
}

/// Yellow 注意報/情報 banner pinned to the bottom of the standby screen, with the
/// text scrolling right→left (telop / marquee), like the real receiver.
fn advisory_banner(ui: &egui::Ui, rect: egui::Rect, advisories: &[String]) {
    if advisories.is_empty() {
        return;
    }
    let bar_h = (rect.height() * 0.08).max(40.0);
    let bar = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - bar_h), rect.max);
    let p = ui.painter();
    p.rect_filled(bar, 0.0, sev_color(Severity::Advisory));

    let ink = Color32::from_rgb(0x1a, 0x1a, 0x1c);
    let text = format!("注意・情報　{}　　　", advisories.join("　／　"));
    let font = FontId::proportional((bar_h * 0.45).min(28.0));
    let galley = ui.fonts(|f| f.layout_no_wrap(text, font, ink));
    let tw = galley.size().x;
    let y = bar.center().y - galley.size().y / 2.0;

    // Scroll one full loop (text width + bar width) at a steady speed.
    let speed = (bar_h * 4.0).max(120.0) as f64; // px/sec
    let total = (tw + bar.width()).max(1.0) as f64;
    let off = (ui.input(|i| i.time) * speed) % total;
    let x = bar.right() - off as f32;

    let clipped = p.with_clip_rect(bar);
    clipped.galley(egui::pos2(x, y), galley, ink);
    ui.ctx().request_repaint(); // keep the marquee animating
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
    p.circle_filled(center + egui::vec2(-w * 0.12, -radius * 0.18), radius * 0.34, Color32::from_rgb(0x0e, 0x33, 0x55));
    p.circle_filled(center + egui::vec2(w * 0.16, -radius * 0.12), radius * 0.22, Color32::from_rgb(0x10, 0x3a, 0x42));
    p.circle_stroke(center, radius + 1.0, egui::Stroke::new(3.0, Color32::from_rgb(0x36, 0x8f, 0xe0)));
    p.circle_stroke(center, radius + 6.0, egui::Stroke::new(8.0, Color32::from_rgba_unmultiplied(0x36, 0x8f, 0xe0, 40)));
    let _ = h;
}

/// Full texture UV (0,0)-(1,1).
const UV_FULL: egui::Rect = egui::Rect {
    min: egui::Pos2 { x: 0.0, y: 0.0 },
    max: egui::Pos2 { x: 1.0, y: 1.0 },
};

/// Aspect-fit (letterbox) an image of size `img` centered within `outer`.
fn fit_contain(outer: egui::Rect, img: egui::Vec2) -> egui::Rect {
    if img.x <= 0.0 || img.y <= 0.0 {
        return outer;
    }
    let scale = (outer.width() / img.x).min(outer.height() / img.y);
    egui::Rect::from_center_size(outer.center(), egui::vec2(img.x * scale, img.y * scale))
}

/// Floating "確認 (ACK)" button shown over a full-screen alert (top-right).
/// Returns true when pressed.
fn ack_button(ctx: &egui::Context) -> bool {
    let mut clicked = false;
    egui::Area::new(egui::Id::new("ack_btn"))
        .anchor(egui::Align2::RIGHT_TOP, [-24.0, 24.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let btn = egui::Button::new(RichText::new("確認 (ACK)").size(22.0).strong().color(Color32::WHITE))
                .fill(Color32::from_rgb(0x10, 0x10, 0x14))
                .stroke(egui::Stroke::new(2.0, Color32::WHITE))
                .min_size(egui::vec2(170.0, 56.0));
            clicked = ui.add(btn).clicked();
        });
    clicked
}

/// Ribbon shown on the standby screen while an acknowledged alert is still in
/// force. Returns true when 再表示 is pressed.
fn ack_ribbon(ctx: &egui::Context, p: &AlertView) -> bool {
    let mut reshow = false;
    egui::Area::new(egui::Id::new("ack_ribbon"))
        .anchor(egui::Align2::CENTER_TOP, [0.0, 16.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(Color32::from_rgb(0x9a, 0x0e, 0x1a))
                .rounding(8.0)
                .inner_margin(egui::Margin::symmetric(18.0, 10.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("● {} を確認済み（継続中）", p.heading()))
                                .size(18.0)
                                .strong()
                                .color(Color32::WHITE),
                        );
                        ui.add_space(8.0);
                        reshow = ui.button("再表示").clicked();
                    });
                });
        });
    reshow
}
