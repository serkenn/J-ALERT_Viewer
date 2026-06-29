//! GUI: the eframe application shell plus the kiosk display and the ported
//! legacy management screens.

mod admin;
mod assets;
mod display;
#[cfg(feature = "audio")]
mod sound;

use chrono::{DateTime, Local, TimeZone};
use egui::Color32;
use jalert_receiver::model::{Category, Severity};
use jalert_receiver::source::{SourceConfig, SourceCtl};
use jalert_receiver::state::AppState;
use jalert_receiver::web::WebHandle;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Initial web-server settings handed from the CLI/env to the app.
pub struct WebInit {
    pub enabled: bool,
    pub port: u16,
    pub cloudflared: bool,
    pub cloudflared_bin: String,
}

#[derive(PartialEq, Clone, Copy)]
pub enum View {
    Display, // 表示 (kiosk)
    Admin,   // 管理 (従来機の web 管理画面の移植)
}

/// Tabs of the ported management screen, mirroring the legacy receiver's web
/// controllers (Top / SystemStatus / Alerts / ExtInterfaceRules /
/// SiteConnectTest / broadcast-link status).
#[derive(PartialEq, Clone, Copy)]
pub enum AdminTab {
    Top,
    VirtualPanel,
    SystemStatus,
    Alerts,
    Rules,
    ConnectTest,
    Cwsd,
}

/// Login user types, matching the legacy receiver's `SystemConfig#authenticate`
/// (a user-type code + password). Default passwords come from the seed config.
#[derive(PartialEq, Clone, Copy)]
pub enum Role {
    Sysadm,   // システム管理者
    Operator, // 運用管理者
    User,     // 一般利用者
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Role::Sysadm => "システム管理者",
            Role::Operator => "運用管理者",
            Role::User => "一般利用者",
        }
    }
    /// Seed defaults from the legacy configuration.
    fn default_password(self) -> &'static str {
        match self {
            Role::Sysadm => "jl10ad",
            Role::Operator => "opjl10",
            Role::User => "usjl10",
        }
    }
    pub(crate) fn authenticate(self, password: &str) -> bool {
        password == self.default_password()
    }
    /// Operator privileges (運用管理): sysadm and operator qualify.
    pub fn is_operator(self) -> bool {
        matches!(self, Role::Sysadm | Role::Operator)
    }
    pub const ALL: [Role; 3] = [Role::Sysadm, Role::Operator, Role::User];
}

#[derive(PartialEq, Clone, Copy)]
pub enum ThemePref {
    System,
    Light,
    Dark,
}

/// Standby-screen styles. パチモン is the stylised homage; リアル aims to
/// reproduce the legacy receiver's idle screen faithfully.
#[derive(PartialEq, Clone, Copy)]
pub enum StandbyStyle {
    Simple,   // 時計＋異常なし
    Pachimon, // パチモン: J-ALERT ロゴ＋地球背景のオマージュ
    Real,     // リアル: 実機の待機画面を忠実再現
}

pub struct App {
    pub state: Arc<Mutex<AppState>>,
    pub source: Arc<SourceCtl>,
    pub view: View,
    pub theme: ThemePref,
    pub fullscreen: bool,
    pub standby_style: StandbyStyle,
    // management screen
    pub admin_tab: AdminTab,
    pub logged_in: bool,
    pub current_role: Role,
    pub login_role: Role,
    pub login_pass: String,
    pub login_err: Option<String>,
    pub selected: Option<u64>,
    pub show_xml: bool,
    pub status_query: String,
    // settings dialog
    pub show_settings: bool,
    pub cfg_host: String,
    pub cfg_port: String,
    // web server (managed at runtime from settings)
    pub web_handle: Option<WebHandle>,
    pub web_enabled: bool,
    pub web_cloudflared: bool,
    pub cloudflared_bin: String,
    pub cfg_web_port: String,
    pub web_err: Option<String>,
    // bundled real screen backgrounds (リアル style)
    screens: assets::Screens,
    // audio
    pub sound_on: bool,
    #[cfg(feature = "audio")]
    sound: Option<sound::Sound>,
    last_alert_key: Option<String>,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        state: Arc<Mutex<AppState>>,
        src_cfg: SourceConfig,
        fullscreen: bool,
        web: WebInit,
    ) -> Self {
        install_fonts(&cc.egui_ctx);
        let screens = assets::Screens::load(&cc.egui_ctx);

        // Wake the UI whenever the feed changes.
        let ctx = cc.egui_ctx.clone();
        let on_change: jalert_receiver::source::OnChange = Arc::new(move || ctx.request_repaint());
        let host = src_cfg.host.clone();
        let port = src_cfg.port;
        let source = jalert_receiver::source::spawn(src_cfg, state.clone(), on_change);

        let mut app = App {
            state,
            source,
            view: View::Display,
            theme: ThemePref::System,
            fullscreen,
            standby_style: StandbyStyle::Simple,
            admin_tab: AdminTab::Top,
            logged_in: false,
            current_role: Role::User,
            login_role: Role::Sysadm,
            login_pass: String::new(),
            login_err: None,
            selected: None,
            show_xml: false,
            status_query: String::new(),
            show_settings: false,
            cfg_host: host,
            cfg_port: port.to_string(),
            web_handle: None,
            web_enabled: web.enabled,
            web_cloudflared: web.cloudflared,
            cloudflared_bin: web.cloudflared_bin,
            cfg_web_port: web.port.to_string(),
            web_err: None,
            screens,
            sound_on: true,
            #[cfg(feature = "audio")]
            sound: sound::Sound::new(),
            last_alert_key: None,
        };
        if web.enabled {
            app.apply_web();
        }
        app
    }

    /// (Re)start or stop the web server to match the current settings.
    fn apply_web(&mut self) {
        if let Some(h) = self.web_handle.take() {
            h.stop();
        }
        self.web_err = None;
        if !self.web_enabled {
            return;
        }
        let port = match self.cfg_web_port.trim().parse::<u16>() {
            Ok(p) => p,
            Err(_) => {
                self.web_err = Some("ポートが不正です".into());
                self.web_enabled = false;
                return;
            }
        };
        match jalert_receiver::web::start(self.state.clone(), port, self.web_cloudflared, &self.cloudflared_bin) {
            Ok(h) => self.web_handle = Some(h),
            Err(e) => {
                self.web_err = Some(format!("起動失敗: {e}"));
                self.web_enabled = false;
            }
        }
    }

    fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
    }

    /// Play the chime + announcement for a category, honoring the sound toggle.
    /// A no-op unless built with the `audio` feature and an output device exists.
    pub(crate) fn play_alert_sound(&self, _category: Category) {
        if !self.sound_on {
            return;
        }
        #[cfg(feature = "audio")]
        if let Some(s) = &self.sound {
            s.play_alert(_category);
        }
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
        self.settings_window(ctx);

        match self.view {
            View::Display => self.show_display(ctx),
            View::Admin => self.show_admin(ctx),
        }
    }
}

impl App {
    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.selectable_value(&mut self.view, View::Display, "🖥 表示");
                ui.selectable_value(&mut self.view, View::Admin, "🛠 管理");

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
                    if ui.button("⚙ 設定").clicked() {
                        if !self.show_settings {
                            let (h, p) = self.source.endpoint();
                            self.cfg_host = h;
                            self.cfg_port = p.to_string();
                        }
                        self.show_settings = !self.show_settings;
                    }
                    if self.view == View::Admin && self.logged_in {
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
                        if ui.button("ログアウト").clicked() {
                            self.logged_in = false;
                        }
                        ui.weak(self.current_role.label());
                    }
                });
            });
        });
    }
}

impl App {
    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        egui::Window::new("設定")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(320.0);
                let (connected, source) = {
                    let st = self.state.lock().unwrap();
                    (st.receiver.connected, st.receiver.source.clone())
                };
                ui.label(egui::RichText::new("JSONL 受信元 (SDR# プラグイン)").strong());
                ui.add_space(6.0);

                if self.source.is_replay() {
                    ui.label(egui::RichText::new("再生(replay)モードのため変更できません").weak());
                } else {
                    egui::Grid::new("settings_grid").num_columns(2).spacing([8.0, 8.0]).show(ui, |ui| {
                        ui.label("ホスト / IP");
                        ui.add(egui::TextEdit::singleline(&mut self.cfg_host).desired_width(180.0).hint_text("127.0.0.1"));
                        ui.end_row();
                        ui.label("ポート");
                        ui.add(egui::TextEdit::singleline(&mut self.cfg_port).desired_width(90.0).hint_text("7355"));
                        ui.end_row();
                    });
                    ui.add_space(8.0);
                    let port_ok = self.cfg_port.trim().parse::<u16>().is_ok();
                    let host_ok = !self.cfg_host.trim().is_empty();
                    ui.horizontal(|ui| {
                        if ui.add_enabled(port_ok && host_ok, egui::Button::new("適用して再接続")).clicked() {
                            if let Ok(p) = self.cfg_port.trim().parse::<u16>() {
                                self.source.set_endpoint(self.cfg_host.trim().to_string(), p);
                            }
                        }
                        if !port_ok {
                            ui.colored_label(Color32::from_rgb(0xe6, 0x00, 0x12), "ポートが不正");
                        }
                    });
                }

                ui.separator();
                ui.label(egui::RichText::new("待機画面スタイル").strong());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.standby_style, StandbyStyle::Simple, "シンプル");
                    ui.selectable_value(&mut self.standby_style, StandbyStyle::Pachimon, "パチモン");
                    ui.selectable_value(&mut self.standby_style, StandbyStyle::Real, "リアル");
                });
                ui.weak(match self.standby_style {
                    StandbyStyle::Simple => "時計＋「異常なし」のみのシンプル表示",
                    StandbyStyle::Pachimon => "J-ALERT ロゴ＋地球背景のオマージュ",
                    StandbyStyle::Real => "従来機の実画面を忠実再現（待機・アラート）",
                });

                ui.separator();
                ui.label(egui::RichText::new("音声").strong());
                ui.add_space(4.0);
                ui.checkbox(&mut self.sound_on, "アラート時にチャイム・読み上げを再生する");
                #[cfg(not(feature = "audio"))]
                ui.weak("（このビルドは音声機能なし）");

                ui.separator();
                ui.label(egui::RichText::new("Web サーバ (ブラウザ/遠隔表示)").strong());
                ui.add_space(4.0);
                let mut dirty = false;
                dirty |= ui.checkbox(&mut self.web_enabled, "Web サーバを起動する").changed();
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(self.web_enabled, |ui| {
                        ui.label("ポート");
                        ui.add(egui::TextEdit::singleline(&mut self.cfg_web_port).desired_width(90.0).hint_text("8080"));
                    });
                });
                dirty |= ui
                    .add_enabled(self.web_enabled, egui::Checkbox::new(&mut self.web_cloudflared, "cloudflared で外部公開 (要インストール)"))
                    .changed();
                ui.horizontal(|ui| {
                    if ui.button("Web設定を適用").clicked() {
                        self.apply_web();
                    }
                    if dirty {
                        ui.weak("（適用で反映）");
                    }
                });
                if let Some(err) = &self.web_err {
                    ui.colored_label(Color32::from_rgb(0xe6, 0x00, 0x12), err);
                } else if let Some(h) = &self.web_handle {
                    ui.label(egui::RichText::new(format!("● 起動中  http://localhost:{}/  (表示)  /admin (管理)", h.port)).color(Color32::from_rgb(0x27, 0xd0, 0x7a)));
                    if h.cloudflared_on {
                        ui.weak("cloudflared 公開URLはコンソール/ログに表示されます");
                    }
                }

                ui.separator();
                ui.horizontal(|ui| {
                    let (dot, label) = if connected {
                        (Color32::from_rgb(0x27, 0xd0, 0x7a), "接続中")
                    } else {
                        (Color32::from_rgb(0xe6, 0x00, 0x12), "未接続")
                    };
                    ui.colored_label(dot, "●");
                    ui.label(format!("{label}  ({source})"));
                });
            });
        if !open {
            self.show_settings = false;
        }
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

/// Accent colour per information category (used in the management screens).
pub fn cat_color(c: Category) -> Color32 {
    match c {
        Category::CivilProtection => Color32::from_rgb(0x9a, 0x0e, 0x8e),
        Category::EmergencyContact => Color32::from_rgb(0xb5, 0x3a, 0x8e),
        Category::Eew => Color32::from_rgb(0xe6, 0x00, 0x12),
        Category::Tsunami => Color32::from_rgb(0xd5, 0x4f, 0x00),
        Category::Volcano => Color32::from_rgb(0xb0, 0x3a, 0x2e),
        Category::Earthquake | Category::SeismicIntensity => Color32::from_rgb(0xc7, 0x8a, 0x00),
        Category::Weather => Color32::from_rgb(0x1f, 0x6f, 0xb2),
        Category::Test => Color32::from_rgb(0x4a, 0x8a, 0x4a),
        Category::Other => Color32::from_rgb(0x76, 0x76, 0x80),
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

#[allow(non_snake_case)]
pub fn Align2_LEFT_CENTER() -> egui::Align2 {
    egui::Align2([egui::Align::Min, egui::Align::Center])
}
