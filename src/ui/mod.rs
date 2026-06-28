//! GUI: the eframe application shell plus the two views.

mod display;
mod inbox;

use chrono::{DateTime, Local, TimeZone};
use egui::Color32;
use jalert_receiver::model::Severity;
use jalert_receiver::source::SourceConfig;
use jalert_receiver::state::AppState;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(PartialEq, Clone, Copy)]
pub enum View {
    Display,
    Inbox,
}

#[derive(PartialEq, Clone, Copy)]
pub enum ThemePref {
    System,
    Light,
    Dark,
}

pub struct App {
    pub state: Arc<Mutex<AppState>>,
    pub view: View,
    pub theme: ThemePref,
    pub fullscreen: bool,
    pub selected: Option<u64>,
    pub show_xml: bool,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        state: Arc<Mutex<AppState>>,
        src_cfg: SourceConfig,
        fullscreen: bool,
    ) -> Self {
        install_fonts(&cc.egui_ctx);

        // Wake the UI whenever the feed changes.
        let ctx = cc.egui_ctx.clone();
        let on_change: jalert_receiver::source::OnChange = Arc::new(move || ctx.request_repaint());
        jalert_receiver::source::spawn(src_cfg, state.clone(), on_change);

        App {
            state,
            view: View::Display,
            theme: ThemePref::System,
            fullscreen,
            selected: None,
            show_xml: false,
        }
    }

    fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Keep the clock and alert flashing live.
        ctx.request_repaint_after(Duration::from_millis(200));

        // Keyboard: F11 toggles fullscreen, Esc leaves it.
        if ctx.input(|i| i.key_pressed(egui::Key::F11)) {
            self.toggle_fullscreen(ctx);
        }
        if self.fullscreen && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.toggle_fullscreen(ctx);
        }

        // Hide all chrome when displaying full-screen; otherwise show the bar.
        let chromeless = self.fullscreen && self.view == View::Display;
        if !chromeless {
            self.top_bar(ctx);
        }

        match self.view {
            View::Display => self.show_display(ctx),
            View::Inbox => self.show_inbox(ctx),
        }
    }
}

impl App {
    fn top_bar(&mut self, ctx: &egui::Context) {
        // The bar itself reads cleanly in dark; force a neutral dark visual here.
        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.selectable_value(&mut self.view, View::Display, "🖥 表示");
                ui.selectable_value(&mut self.view, View::Inbox, "📥 受信箱");

                let (connected, unread, source) = {
                    let st = self.state.lock().unwrap();
                    (st.receiver.connected, st.unread(), st.receiver.source.clone())
                };
                ui.separator();
                let (dot, label) = if connected {
                    (Color32::from_rgb(0x27, 0xd0, 0x7a), "接続中")
                } else {
                    (Color32::from_rgb(0xe6, 0x00, 0x12), "切断")
                };
                ui.colored_label(dot, "●");
                ui.label(label);
                ui.label(egui::RichText::new(source).weak());
                if unread > 0 {
                    ui.label(
                        egui::RichText::new(format!(" 未読 {unread} "))
                            .background_color(Color32::from_rgb(0xe6, 0x00, 0x12))
                            .color(Color32::WHITE),
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let fs = if self.fullscreen { "⤡ 全画面解除" } else { "⤢ 全画面" };
                    if ui.button(fs).clicked() {
                        self.toggle_fullscreen(ctx);
                    }
                    if self.view == View::Inbox {
                        egui::ComboBox::from_id_salt("theme")
                            .selected_text(match self.theme {
                                ThemePref::System => "テーマ:自動",
                                ThemePref::Light => "テーマ:ライト",
                                ThemePref::Dark => "テーマ:ダーク",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.theme, ThemePref::System, "自動");
                                ui.selectable_value(&mut self.theme, ThemePref::Light, "ライト");
                                ui.selectable_value(&mut self.theme, ThemePref::Dark, "ダーク");
                            });
                        if ui.button("すべて既読").clicked() {
                            self.state.lock().unwrap().mark_all_read();
                        }
                    }
                });
            });
        });
    }
}

// ---- shared helpers ----

pub fn sev_color(s: Severity) -> Color32 {
    match s {
        Severity::Emergency => Color32::from_rgb(0x9a, 0x0e, 0x8e),
        Severity::Warning => Color32::from_rgb(0xce, 0x00, 0x12),
        Severity::Advisory => Color32::from_rgb(0xf2, 0xc2, 0x00),
        Severity::None => Color32::from_rgb(0x76, 0x76, 0x80),
    }
}

/// Ink color that reads on top of `sev_color`.
pub fn sev_ink(s: Severity) -> Color32 {
    match s {
        Severity::Advisory => Color32::from_rgb(0x1a, 0x1a, 0x1c),
        _ => Color32::WHITE,
    }
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "jp".to_owned(),
        egui::FontData::from_static(include_bytes!("../../assets/font/MPLUS1p-Regular.ttf")),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "jp".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push("jp".to_owned());
    ctx.set_fonts(fonts);
}

pub fn local_from_ms(ms: i64) -> Option<DateTime<Local>> {
    Local.timestamp_millis_opt(ms).single()
}

pub fn hms(ms: i64) -> String {
    local_from_ms(ms).map(|d| d.format("%H:%M:%S").to_string()).unwrap_or_else(|| "—".into())
}

pub fn md_hms(ms: i64) -> String {
    local_from_ms(ms).map(|d| d.format("%m/%d %H:%M:%S").to_string()).unwrap_or_else(|| "—".into())
}

pub fn report_fmt(iso: &str) -> String {
    DateTime::parse_from_rfc3339(iso)
        .map(|d| d.format("%Y/%m/%d %H:%M").to_string())
        .unwrap_or_else(|_| if iso.is_empty() { "—".into() } else { iso.to_string() })
}
