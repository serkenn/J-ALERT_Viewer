//! Management mailbox: read/unread list + detail, styled after the Digital
//! Agency Design System (light/dark, calm, accessible).

use super::{md_hms, report_fmt, sev_color, sev_ink, App, ThemePref};
use egui::{Color32, FontId, RichText, Sense, Stroke};
use jalert_receiver::model::Severity;

struct Row {
    id: u64,
    severity: Severity,
    label: String,
    area: String,
    kinds: String,
    info_type: String,
    rx_time_ms: i64,
    read: bool,
}

struct Detail {
    id: u64,
    severity: Severity,
    label: String,
    title: String,
    area: String,
    info_type: String,
    report_time: String,
    rx_time_ms: i64,
    packet_time: String,
    kinds: Vec<String>,
    headline: String,
    read: bool,
    xml: String,
}

impl App {
    pub(crate) fn show_inbox(&mut self, ctx: &egui::Context) {
        apply_theme(ctx, self.theme);

        let rows: Vec<Row> = {
            let st = self.state.lock().unwrap();
            st.inbox()
                .map(|it| Row {
                    id: it.id,
                    severity: it.severity,
                    label: it.severity.label().to_string(),
                    area: if it.area_name.is_empty() { it.head_title.clone() } else { it.area_name.clone() },
                    kinds: if it.kinds.is_empty() { it.headline.clone() } else { it.kinds.join("・") },
                    info_type: it.info_type.clone(),
                    rx_time_ms: it.rx_time_ms,
                    read: it.read,
                })
                .collect()
        };

        let mut clicked: Option<u64> = None;
        let mut toggle: Option<(u64, bool)> = None;

        egui::SidePanel::left("inbox_list")
            .resizable(true)
            .default_width(420.0)
            .width_range(320.0..=560.0)
            .show(ctx, |ui| {
                if rows.is_empty() {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| ui.weak("受信した電文はまだありません"));
                    return;
                }
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for row in &rows {
                        if list_row(ui, row, self.selected == Some(row.id)).clicked() {
                            clicked = Some(row.id);
                        }
                    }
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let detail = self.selected.and_then(|id| self.load_detail(id));
            match detail {
                Some(d) => {
                    if let Some(act) = detail_pane(ui, &d, &mut self.show_xml) {
                        toggle = Some(act);
                    }
                }
                None => {
                    ui.add_space(ui.available_height() * 0.35);
                    ui.vertical_centered(|ui| ui.weak("左の一覧から項目を選択してください"));
                }
            }
        });

        if let Some(id) = clicked {
            self.selected = Some(id);
            self.show_xml = false;
            self.state.lock().unwrap().mark_read(id, true);
        }
        if let Some((id, read)) = toggle {
            self.state.lock().unwrap().mark_read(id, read);
        }
    }

    fn load_detail(&self, id: u64) -> Option<Detail> {
        let st = self.state.lock().unwrap();
        let it = st.item(id)?;
        Some(Detail {
            id: it.id,
            severity: it.severity,
            label: it.severity.label().to_string(),
            title: it.title.clone(),
            area: if it.area_name.is_empty() { it.head_title.clone() } else { it.area_name.clone() },
            info_type: it.info_type.clone(),
            report_time: it.report_time.clone(),
            rx_time_ms: it.rx_time_ms,
            packet_time: it.packet_time.clone(),
            kinds: it.kinds.clone(),
            headline: it.headline.clone(),
            read: it.read,
            xml: it.xml.clone(),
        })
    }
}

fn sev_tag(ui: &mut egui::Ui, s: Severity, label: &str, size: f32) {
    egui::Frame::none()
        .fill(sev_color(s))
        .rounding(6.0)
        .inner_margin(egui::Margin::symmetric(9.0, 2.0))
        .show(ui, |ui| {
            ui.label(RichText::new(label).font(FontId::proportional(size)).strong().color(sev_ink(s)));
        });
}

fn list_row(ui: &mut egui::Ui, row: &Row, selected: bool) -> egui::Response {
    let accent = ui.visuals().selection.bg_fill;
    let fill = if selected { accent } else { Color32::TRANSPARENT };
    let inner = egui::Frame::none()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(14.0, 10.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                // severity color bar
                let (rect, _) = ui.allocate_exact_size(egui::vec2(4.0, 38.0), Sense::hover());
                ui.painter().rect_filled(rect, 2.0, sev_color(row.severity));
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        sev_tag(ui, row.severity, &row.label, 12.0);
                        let area = RichText::new(&row.area);
                        ui.label(if row.read { area } else { area.strong() });
                        if !row.info_type.is_empty() {
                            ui.weak(RichText::new(&row.info_type).size(12.0));
                        }
                    });
                    ui.label(RichText::new(&row.kinds).size(13.0).weak());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.weak(RichText::new(md_hms(row.rx_time_ms)).size(12.0));
                    if !row.read {
                        ui.colored_label(ui.visuals().hyperlink_color, "●");
                    }
                });
            });
        });
    let resp = ui.interact(inner.response.rect, ui.make_persistent_id(("row", row.id)), Sense::click());
    if resp.hovered() && !selected {
        ui.painter().rect_filled(inner.response.rect, 0.0, ui.visuals().widgets.hovered.bg_fill.linear_multiply(0.3));
    }
    ui.separator();
    resp
}

/// Returns Some((id, new_read)) if the read/unread button was pressed.
fn detail_pane(ui: &mut egui::Ui, d: &Detail, show_xml: &mut bool) -> Option<(u64, bool)> {
    let mut action = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            sev_tag(ui, d.severity, &d.label, 15.0);
            if !d.title.is_empty() {
                ui.weak(&d.title);
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new(&d.area).font(FontId::proportional(30.0)).strong());
        ui.add_space(8.0);
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            meta(ui, "発表種別", if d.info_type.is_empty() { "—" } else { &d.info_type });
            meta(ui, "発表時刻", &report_fmt(&d.report_time));
            meta(ui, "受信時刻", &md_hms(d.rx_time_ms));
            meta(ui, "電文時刻", if d.packet_time.is_empty() { "—" } else { &d.packet_time });
        });
        ui.separator();
        ui.add_space(10.0);

        if !d.kinds.is_empty() {
            ui.label(RichText::new("発表中の種別").size(13.0).weak());
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                for k in &d.kinds {
                    egui::Frame::none()
                        .stroke(Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color))
                        .rounding(8.0)
                        .inner_margin(egui::Margin::symmetric(15.0, 7.0))
                        .show(ui, |ui| ui.label(RichText::new(k).size(18.0).strong()));
                    ui.add_space(8.0);
                }
            });
            ui.add_space(16.0);
        }

        if !d.headline.is_empty() {
            ui.label(RichText::new("本文").size(13.0).weak());
            ui.add_space(6.0);
            egui::Frame::none()
                .fill(ui.visuals().faint_bg_color)
                .rounding(8.0)
                .inner_margin(egui::Margin::same(14.0))
                .show(ui, |ui| ui.label(RichText::new(&d.headline).size(18.0)));
            ui.add_space(18.0);
        }

        ui.horizontal(|ui| {
            let xml_label = if *show_xml { "XML原文を隠す" } else { "XML原文を表示" };
            if ui.button(xml_label).clicked() {
                *show_xml = !*show_xml;
            }
            let read_label = if d.read { "未読に戻す" } else { "既読にする" };
            if ui.button(read_label).clicked() {
                action = Some((d.id, !d.read));
            }
        });

        if *show_xml {
            ui.add_space(10.0);
            egui::Frame::none()
                .fill(Color32::from_rgb(0x1a, 0x1a, 0x1c))
                .rounding(8.0)
                .inner_margin(egui::Margin::same(12.0))
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&d.xml).font(FontId::monospace(12.5)).color(Color32::from_rgb(0xd0, 0xdd, 0xee)),
                        )
                        .wrap(),
                    );
                });
        }
    });
    action
}

fn meta(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.weak(RichText::new(label).size(13.0));
        ui.label(RichText::new(value).size(13.0));
        ui.add_space(18.0);
    });
}

fn apply_theme(ctx: &egui::Context, pref: ThemePref) {
    let dark = match pref {
        ThemePref::Dark => true,
        ThemePref::Light => false,
        ThemePref::System => ctx.style().visuals.dark_mode,
    };
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    if dark {
        v.panel_fill = Color32::from_rgb(0x16, 0x18, 0x1d);
        v.override_text_color = Some(Color32::from_rgb(0xe7, 0xe8, 0xea));
        v.hyperlink_color = Color32::from_rgb(0x8a, 0xa0, 0xff);
        v.selection.bg_fill = Color32::from_rgb(0x22, 0x27, 0x3a);
        v.faint_bg_color = Color32::from_rgb(0x1c, 0x1f, 0x25);
    } else {
        v.panel_fill = Color32::WHITE;
        v.override_text_color = Some(Color32::from_rgb(0x1a, 0x1a, 0x1c));
        v.hyperlink_color = Color32::from_rgb(0x00, 0x17, 0xc1);
        v.selection.bg_fill = Color32::from_rgb(0xd6, 0xe0, 0xff);
        v.faint_bg_color = Color32::from_rgb(0xf4, 0xf4, 0xf6);
    }
    ctx.set_visuals(v);
}
