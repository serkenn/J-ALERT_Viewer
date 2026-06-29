//! The legacy receiver's web 管理画面, ported to a native desktop view. Tabs and
//! login mirror the original Rails controllers (TopController, VirtualPanel,
//! SystemStatus, Alerts, ExtInterfaceRules, SiteConnectTest, CwsdStatus). Login
//! follows `SystemConfig#authenticate` (user type + password).

use super::{cat_color, md_hms, report_fmt, sev_color, sev_ink, AdminTab, App, Role, ThemePref};
use egui::{Color32, FontId, RichText, Sense, Stroke};
use jalert_receiver::model::{AlertType, Category, Severity};

impl App {
    pub(crate) fn show_admin(&mut self, ctx: &egui::Context) {
        apply_theme(ctx, self.theme);
        if !self.logged_in {
            self.show_login(ctx);
            return;
        }

        let operator = self.current_role.is_operator();
        egui::TopBottomPanel::top("admin_tabs").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.admin_tab, AdminTab::Top, "トップ");
                ui.selectable_value(&mut self.admin_tab, AdminTab::VirtualPanel, "仮想パネル");
                ui.selectable_value(&mut self.admin_tab, AdminTab::Alerts, "緊急情報一覧");
                // 受信機状態 / 同報系I/F / 手動発報 は運用管理者以上のみ（operator_required）
                if operator {
                    ui.selectable_value(&mut self.admin_tab, AdminTab::Manual, "手動発報");
                    ui.selectable_value(&mut self.admin_tab, AdminTab::SystemStatus, "受信機状態");
                    ui.selectable_value(&mut self.admin_tab, AdminTab::Cwsd, "同報系I/F");
                }
                ui.selectable_value(&mut self.admin_tab, AdminTab::Rules, "外部IF動作ルール");
                ui.selectable_value(&mut self.admin_tab, AdminTab::ConnectTest, "接続テスト");
            });
            ui.add_space(2.0);
        });

        // Guard operator-only screens if the role changed.
        if matches!(self.admin_tab, AdminTab::SystemStatus | AdminTab::Cwsd | AdminTab::Manual) && !operator {
            self.admin_tab = AdminTab::Top;
        }

        match self.admin_tab {
            AdminTab::Top => self.show_top(ctx),
            AdminTab::VirtualPanel => self.show_virtual_panel(ctx),
            AdminTab::Manual => self.show_manual(ctx),
            AdminTab::SystemStatus => self.show_system_status(ctx),
            AdminTab::Alerts => self.show_alerts(ctx),
            AdminTab::Rules => self.show_rules(ctx),
            AdminTab::ConnectTest => self.show_connect_test(ctx),
            AdminTab::Cwsd => self.show_cwsd(ctx),
        }
    }

    fn show_login(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(ui.available_height() * 0.18);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("受信機 管理画面").size(26.0).strong());
                ui.add_space(4.0);
                ui.weak("ユーザ種別を選び、パスワードを入力してください");
                ui.add_space(18.0);
                egui::Frame::group(ui.style()).inner_margin(egui::Margin::same(18.0)).show(ui, |ui| {
                    ui.set_width(320.0);
                    egui::Grid::new("login_grid").num_columns(2).spacing([10.0, 10.0]).show(ui, |ui| {
                        ui.label("ユーザ種別");
                        egui::ComboBox::from_id_salt("login_role")
                            .selected_text(self.login_role.label())
                            .show_ui(ui, |ui| {
                                for r in Role::ALL {
                                    ui.selectable_value(&mut self.login_role, r, r.label());
                                }
                            });
                        ui.end_row();
                        ui.label("パスワード");
                        ui.add(egui::TextEdit::singleline(&mut self.login_pass).password(true).desired_width(180.0));
                        ui.end_row();
                    });
                    ui.add_space(12.0);
                    let submit = ui.add_sized([ui.available_width(), 32.0], egui::Button::new("ログイン")).clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if submit {
                        if self.login_role.authenticate(self.login_pass.trim()) {
                            self.logged_in = true;
                            self.current_role = self.login_role;
                            self.login_err = None;
                            self.login_pass.clear();
                            self.admin_tab = AdminTab::Top;
                        } else {
                            self.login_err = Some("ログイン名、パスワードが一致しません".into());
                        }
                    }
                    if let Some(e) = &self.login_err {
                        ui.add_space(8.0);
                        ui.colored_label(Color32::from_rgb(0xe6, 0x00, 0x12), e);
                    }
                    ui.add_space(6.0);
                    ui.weak("初期パスワード: システム管理者 jl10ad / 運用管理者 opjl10 / 一般利用者 usjl10");
                });
            });
        });
    }

    // ---- トップ ----
    fn show_top(&mut self, ctx: &egui::Context) {
        let (connected, source, mode, total, unread, alerts, advisories) = {
            let st = self.state.lock().unwrap();
            let alerts: Vec<String> = st
                .alerts()
                .iter()
                .map(|c| format!("{}　{}", c.category.label(), c.area_label()))
                .collect();
            let advisories: Vec<String> = st
                .advisories()
                .iter()
                .map(|c| format!("{}　{}", c.category.label(), c.area_label()))
                .collect();
            (
                st.receiver.connected,
                st.receiver.source.clone(),
                st.mode().to_string(),
                st.receiver.total_lines,
                st.unread(),
                alerts,
                advisories,
            )
        };

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(8.0);
                ui.heading("トップ");
                ui.add_space(8.0);

                let (mode_txt, mode_col) = match mode.as_str() {
                    "alert" => ("緊急情報を表示中", Color32::from_rgb(0xe6, 0x00, 0x12)),
                    "advisory" => ("注意・情報あり", Color32::from_rgb(0xc7, 0x8a, 0x00)),
                    _ => ("受信待機中（異常なし）", Color32::from_rgb(0x27, 0xa0, 0x5e)),
                };
                card(ui, "現在の状態", |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(mode_col, "●");
                        ui.label(RichText::new(mode_txt).size(18.0).strong());
                    });
                    ui.add_space(4.0);
                    kv(ui, "受信元", &source);
                    kv(ui, "受信", if connected { "接続中" } else { "切断" });
                    kv(ui, "受信総数", &total.to_string());
                    kv(ui, "未読", &unread.to_string());
                });

                ui.add_space(8.0);
                card(ui, "表示中の緊急情報", |ui| {
                    if alerts.is_empty() {
                        ui.weak("なし");
                    } else {
                        for a in &alerts {
                            ui.label(format!("・{a}"));
                        }
                    }
                });

                ui.add_space(8.0);
                card(ui, "注意・情報", |ui| {
                    if advisories.is_empty() {
                        ui.weak("なし");
                    } else {
                        for a in &advisories {
                            ui.label(format!("・{a}"));
                        }
                    }
                });
            });
        });
    }

    // ---- 仮想パネル（フロントパネル）----
    fn show_virtual_panel(&mut self, ctx: &egui::Context) {
        let (connected, last_sat, last_terr, alert) = {
            let st = self.state.lock().unwrap();
            (st.receiver.connected, st.last_sat_ms, st.last_terr_ms, st.mode() == "alert")
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let recent = |ms: i64| ms > 0 && now - ms < 120_000;

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(8.0);
                ui.heading("仮想パネル（フロントパネル）");
                ui.weak("実機のフロントパネル LED を模した表示です。");
                ui.add_space(10.0);

                card(ui, "リンク状態", |ui| {
                    ui.horizontal(|ui| {
                        lamp_dot(ui, if connected { Lit::Green } else { Lit::Off });
                        ui.label(if connected { "接続中" } else { "切断" });
                    });
                });

                ui.add_space(8.0);
                card(ui, "フロントパネル", |ui| {
                    ui.horizontal(|ui| {
                        lamp_col(ui, "Status", if alert { Lit::Red } else if connected { Lit::Green } else { Lit::Off });
                        lamp_col(ui, "衛星系", if recent(last_sat) { Lit::Green } else { Lit::Off });
                        lamp_col(ui, "地上系", if recent(last_terr) { Lit::Green } else { Lit::Off });
                        lamp_col(ui, "アプリ", if alert { Lit::Red } else { Lit::Off });
                        lamp_col(ui, "外部I/F", Lit::Off);
                    });
                });

                ui.add_space(8.0);
                card(ui, "接点出力 (DIO)", |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for i in 1..=8 {
                            lamp_col(ui, &format!("#{i}"), Lit::Off);
                        }
                    });
                });
                ui.weak("※ 接点出力・外部I/F の実ハードは本移植版にはありません。");
            });
        });
    }

    // ---- 手動発報 ----
    fn show_manual(&mut self, ctx: &egui::Context) {
        // (表示名, 情報種別, 電文種別)
        const CATS: &[(&str, Category, AlertType)] = &[
            ("国民保護情報", Category::CivilProtection, AlertType::Jalt),
            ("緊急連絡", Category::EmergencyContact, AlertType::Ifda),
            ("緊急地震速報", Category::Eew, AlertType::Eprq),
            ("地震情報", Category::Earthquake, AlertType::Ioeq),
            ("震度速報", Category::SeismicIntensity, AlertType::Ioeq),
            ("津波情報", Category::Tsunami, AlertType::Issw),
            ("火山情報", Category::Volcano, AlertType::Volc),
            ("気象情報", Category::Weather, AlertType::Wrma),
        ];
        let prefs = super::areas::PREFS;
        if self.man_pref >= prefs.len() {
            self.man_pref = 0;
        }
        if self.man_city > prefs[self.man_pref].cities.len() {
            self.man_city = 0;
        }

        let active: Vec<String> = {
            let st = self.state.lock().unwrap();
            st.alerts()
                .iter()
                .map(|c| format!("{}　{}", c.category.label(), c.area_label()))
                .collect()
        };

        let mut fire: Option<(AlertType, Category, String, String)> = None;
        let mut clear = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(8.0);
                ui.heading("手動発報");
                ui.weak("情報種別と地域（都道府県・市区町村）を選び、画面に緊急情報を発報します（試験・訓練用）。");
                ui.add_space(10.0);

                card(ui, "発報内容", |ui| {
                    let cur = CATS
                        .iter()
                        .find(|(_, c, _)| *c == self.man_category)
                        .map(|(l, _, _)| *l)
                        .unwrap_or("国民保護情報");
                    egui::Grid::new("manual_grid").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
                        ui.label("情報種別");
                        egui::ComboBox::from_id_salt("man_cat").selected_text(cur).width(220.0).show_ui(ui, |ui| {
                            for (label, cat, _) in CATS {
                                ui.selectable_value(&mut self.man_category, *cat, *label);
                            }
                        });
                        ui.end_row();

                        ui.label("都道府県");
                        let pref_resp = egui::ComboBox::from_id_salt("man_pref")
                            .selected_text(prefs[self.man_pref].name)
                            .width(220.0)
                            .show_ui(ui, |ui| {
                                let mut changed = false;
                                for (i, p) in prefs.iter().enumerate() {
                                    if ui.selectable_value(&mut self.man_pref, i, p.name).changed() {
                                        changed = true;
                                    }
                                }
                                changed
                            });
                        if pref_resp.inner == Some(true) {
                            self.man_city = 0; // 都道府県が変わったら市区町村をリセット
                        }
                        ui.end_row();

                        ui.label("市区町村");
                        let cities = prefs[self.man_pref].cities;
                        let city_text = if self.man_city == 0 {
                            "（都道府県全体）"
                        } else {
                            cities[self.man_city - 1].1
                        };
                        egui::ComboBox::from_id_salt("man_city").selected_text(city_text).width(220.0).show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.man_city, 0, "（都道府県全体）");
                            for (i, c) in cities.iter().enumerate() {
                                ui.selectable_value(&mut self.man_city, i + 1, c.1);
                            }
                        });
                        ui.end_row();

                        ui.label("見出し/本文");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.man_headline)
                                .desired_width(280.0)
                                .hint_text("例: 大津波警報 / 噴火警戒レベル5 / 弾道ミサイル発射情報"),
                        );
                        ui.end_row();
                    });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(RichText::new("発報").strong()).min_size(egui::vec2(96.0, 30.0))).clicked() {
                            let pref = &prefs[self.man_pref];
                            let cities = pref.cities;
                            let area = if self.man_city == 0 {
                                pref.name.to_string()
                            } else {
                                format!("{} {}", pref.name, cities[self.man_city - 1].1)
                            };
                            if let Some((_, cat, at)) = CATS.iter().find(|(_, c, _)| *c == self.man_category) {
                                fire = Some((*at, *cat, area, self.man_headline.clone()));
                            }
                        }
                        if ui.add(egui::Button::new("全解除").min_size(egui::vec2(96.0, 30.0))).clicked() {
                            clear = true;
                        }
                    });
                    ui.weak("※ 発報すると表示画面にアラートが出ます。確認(ACK)や「全解除」で復帰します。");
                });

                ui.add_space(8.0);
                card(ui, "発報中の緊急情報", |ui| {
                    if active.is_empty() {
                        ui.weak("なし");
                    } else {
                        for a in &active {
                            ui.label(format!("・{a}"));
                        }
                    }
                });
            });
        });

        if let Some((at, cat, area, head)) = fire {
            self.state.lock().unwrap().inject_manual(at, cat, area, String::new(), head);
        }
        if clear {
            self.state.lock().unwrap().clear_alerts();
            self.acked = None;
        }
    }

    // ---- システム状態 ----
    fn show_system_status(&mut self, ctx: &egui::Context) {
        let (connected, source, total, last_line, last_sat, last_terr, counts) = {
            let st = self.state.lock().unwrap();
            (
                st.receiver.connected,
                st.receiver.source.clone(),
                st.receiver.total_lines,
                st.receiver.last_line_ms,
                st.last_sat_ms,
                st.last_terr_ms,
                st.type_counts(),
            )
        };

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(8.0);
                ui.heading("システム状態");
                ui.add_space(8.0);

                card(ui, "受信機", |ui| {
                    kv(ui, "機種", "J-ALERT 受信表示機");
                    kv(ui, "ファームウェア", env!("CARGO_PKG_VERSION"));
                    kv(ui, "受信元 (SDR# プラグイン)", &source);
                    ui.horizontal(|ui| {
                        ui.weak("受信状態");
                        let (c, t) = if connected {
                            (Color32::from_rgb(0x27, 0xa0, 0x5e), "接続中")
                        } else {
                            (Color32::from_rgb(0xe6, 0x00, 0x12), "切断")
                        };
                        ui.colored_label(c, "●");
                        ui.label(t);
                    });
                });

                ui.add_space(8.0);
                card(ui, "受信系統", |ui| {
                    kv(ui, "最終受信", &md_hms(last_line));
                    kv(ui, "衛星系チャネル 最終受信", &md_hms(last_sat));
                    kv(ui, "地上系チャネル 最終受信", &md_hms(last_terr));
                    kv(ui, "累計受信数", &total.to_string());
                });

                ui.add_space(8.0);
                card(ui, "電文種別別 受信数", |ui| {
                    egui::Grid::new("type_counts").num_columns(3).spacing([16.0, 6.0]).striped(true).show(ui, |ui| {
                        ui.strong("種別コード");
                        ui.strong("情報種別");
                        ui.strong("受信数");
                        ui.end_row();
                        for (t, n) in &counts {
                            ui.monospace(t.code());
                            ui.label(Category::from_alert_type(*t).label());
                            ui.label(n.to_string());
                            ui.end_row();
                        }
                    });
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("緊急情報の検索:");
                    ui.add(egui::TextEdit::singleline(&mut self.status_query).desired_width(220.0).hint_text("地域・種別など"));
                    if ui.button("一覧で検索").clicked() {
                        self.admin_tab = AdminTab::Alerts;
                    }
                });
                ui.weak("※ 検索語は「緊急情報一覧」の絞り込みに使われます");
            });
        });
    }

    // ---- 緊急情報一覧 ----
    fn show_alerts(&mut self, ctx: &egui::Context) {
        let q = self.status_query.trim().to_string();
        let rows: Vec<Row> = {
            let st = self.state.lock().unwrap();
            st.inbox()
                .filter(|it| {
                    if q.is_empty() {
                        return true;
                    }
                    let hay = format!(
                        "{} {} {} {} {}",
                        it.category.label(),
                        it.alert_type.code(),
                        it.area_name,
                        it.head_title,
                        it.kinds.join(" ")
                    );
                    hay.contains(&q)
                })
                .map(|it| Row {
                    id: it.id,
                    severity: it.severity,
                    category: it.category,
                    type_code: it.alert_type.code(),
                    channel: it.channel.label(),
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

        egui::SidePanel::left("alerts_list")
            .resizable(true)
            .default_width(440.0)
            .width_range(340.0..=600.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("検索:");
                    ui.add(egui::TextEdit::singleline(&mut self.status_query).desired_width(180.0).hint_text("地域・種別"));
                    if ui.button("すべて既読").clicked() {
                        self.state.lock().unwrap().mark_all_read();
                    }
                });
                ui.separator();
                if rows.is_empty() {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| ui.weak("該当する電文はありません"));
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

    // ---- 外部インタフェース動作ルール ----
    fn show_rules(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(8.0);
                ui.heading("外部インタフェース動作ルール");
                ui.weak("情報種別ごとに「画面表示」(緊急情報表示設定) と外部出力の動作を設定します。");
                ui.add_space(10.0);

                let mut st = self.state.lock().unwrap();
                egui::Grid::new("rules_grid").num_columns(6).spacing([14.0, 8.0]).striped(true).show(ui, |ui| {
                    ui.strong("情報種別");
                    ui.strong("種別コード");
                    ui.strong("画面表示");
                    ui.strong("音声/鳴動");
                    ui.strong("接点出力");
                    ui.strong("同報系連携");
                    ui.end_row();
                    for r in &mut st.rules {
                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(6.0, 16.0), Sense::hover());
                            ui.painter().rect_filled(rect, 1.5, cat_color(r.category));
                            ui.label(r.category.label());
                        });
                        ui.monospace(r.alert_type.code());
                        ui.checkbox(&mut r.display, "");
                        ui.checkbox(&mut r.sound, "");
                        ui.checkbox(&mut r.contact_out, "");
                        ui.checkbox(&mut r.cwsd, "");
                        ui.end_row();
                    }
                });
                ui.add_space(10.0);
                ui.weak("※「画面表示」を外すと、その種別は受信しても全画面表示／バナーに出ません（一覧には記録されます）。");
            });
        });
    }

    // ---- 接続テスト ----
    fn show_connect_test(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            ui.heading("接続テスト");
            ui.add_space(8.0);
            card(ui, "疎通確認", |ui| {
                ui.label("受信元 (SDR# プラグイン) への疎通を確認します。");
                ui.weak("※ 本移植版は WAN/衛星への接続を持たないため、SDR# 受信元の状態を確認します。");
                ui.add_space(8.0);
                if ui.button("テスト実行").clicked() {
                    self.state.lock().unwrap().run_connect_test();
                }
            });
            ui.add_space(8.0);
            let result = self.state.lock().unwrap().connect_test.clone();
            if let Some(r) = result {
                card(ui, "結果", |ui| {
                    let (c, t) = if r.ok {
                        (Color32::from_rgb(0x27, 0xa0, 0x5e), "成功")
                    } else {
                        (Color32::from_rgb(0xe6, 0x00, 0x12), "失敗")
                    };
                    ui.horizontal(|ui| {
                        ui.colored_label(c, "●");
                        ui.label(RichText::new(t).strong());
                        ui.weak(md_hms(r.at_ms));
                    });
                    ui.add_space(4.0);
                    ui.label(r.message);
                });
            } else {
                ui.weak("まだテストを実行していません。");
            }
        });
    }

    // ---- 同報系I/F状態 ----
    fn show_cwsd(&mut self, ctx: &egui::Context) {
        let c = self.state.lock().unwrap().cwsd.clone();
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            ui.heading("同報系（防災行政無線）I/F状態");
            ui.weak("実機の外部インタフェースに相当します。本移植版では接続ハードが無いため模擬表示です。");
            ui.add_space(10.0);
            card(ui, "リンク状態", |ui| {
                ui.horizontal(|ui| {
                    let (col, t) = if c.connected {
                        (Color32::from_rgb(0x27, 0xa0, 0x5e), "接続中")
                    } else {
                        (Color32::from_rgb(0x76, 0x76, 0x80), "未接続（模擬）")
                    };
                    ui.colored_label(col, "●");
                    ui.label(t);
                });
                ui.add_space(6.0);
                kv(ui, "接続先", &c.host);
                kv(ui, "自局番号", &c.local_no);
                kv(ui, "相手局番号", &c.remote_no);
                kv(ui, "命令コード", &c.command);
                kv(ui, "再生待ち情報数", &c.queue_len.to_string());
                kv(ui, "情報管理リストチェックサム", &c.checksum);
            });
        });
    }

    fn load_detail(&self, id: u64) -> Option<Detail> {
        let st = self.state.lock().unwrap();
        let it = st.item(id)?;
        Some(Detail {
            id: it.id,
            severity: it.severity,
            category: it.category,
            alert_type: it.alert_type,
            allowed: it.allowed,
            channel: it.channel.label(),
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

struct Row {
    id: u64,
    severity: Severity,
    category: Category,
    type_code: &'static str,
    channel: &'static str,
    area: String,
    kinds: String,
    info_type: String,
    rx_time_ms: i64,
    read: bool,
}

struct Detail {
    id: u64,
    severity: Severity,
    category: Category,
    alert_type: AlertType,
    allowed: bool,
    channel: &'static str,
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

// ---- small widgets ----

enum Lit {
    Off,
    Green,
    Red,
}

fn lit_color(l: &Lit) -> Color32 {
    match l {
        Lit::Off => Color32::from_rgba_unmultiplied(0x44, 0x44, 0x55, 125),
        Lit::Green => Color32::from_rgb(0x11, 0xee, 0x22),
        Lit::Red => Color32::from_rgb(0xfe, 0x33, 0x11),
    }
}

fn lamp_dot(ui: &mut egui::Ui, lit: Lit) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 7.0, lit_color(&lit));
}

fn lamp_col(ui: &mut egui::Ui, label: &str, lit: Lit) {
    ui.vertical(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(56.0, 28.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 11.0, lit_color(&lit));
        ui.label(RichText::new(label).size(11.0));
    });
}

fn card(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(14.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width().min(720.0));
            ui.label(RichText::new(title).size(14.0).strong());
            ui.add_space(6.0);
            body(ui);
        });
}

fn kv(ui: &mut egui::Ui, k: &str, v: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{k}：")).weak());
        ui.label(v);
    });
}

fn cat_tag(ui: &mut egui::Ui, c: Category, sev: Severity, size: f32) {
    egui::Frame::none()
        .fill(cat_color(c))
        .rounding(6.0)
        .inner_margin(egui::Margin::symmetric(9.0, 2.0))
        .show(ui, |ui| {
            let label = if c == Category::Weather { sev.label() } else { c.label() };
            ui.label(RichText::new(label).font(FontId::proportional(size)).strong().color(Color32::WHITE));
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
                let (rect, _) = ui.allocate_exact_size(egui::vec2(4.0, 40.0), Sense::hover());
                ui.painter().rect_filled(rect, 2.0, sev_color(row.severity));
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        cat_tag(ui, row.category, row.severity, 12.0);
                        let area = RichText::new(&row.area);
                        ui.label(if row.read { area } else { area.strong() });
                        ui.weak(RichText::new(format!("[{}/{}]", row.type_code, row.channel)).size(11.0));
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
            cat_tag(ui, d.category, d.severity, 15.0);
            if !d.title.is_empty() {
                ui.weak(&d.title);
            }
            if !d.allowed {
                ui.weak(RichText::new("（表示対象外）").color(Color32::from_rgb(0x99, 0x99, 0x99)));
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new(&d.area).font(FontId::proportional(30.0)).strong());
        ui.add_space(8.0);
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            meta(ui, "情報種別", d.category.label());
            meta(ui, "電文種別", d.alert_type.code());
            meta(ui, "受信系統", d.channel);
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
            if !d.xml.is_empty() {
                let xml_label = if *show_xml { "XML原文を隠す" } else { "XML原文を表示" };
                if ui.button(xml_label).clicked() {
                    *show_xml = !*show_xml;
                }
            }
            let read_label = if d.read { "未読に戻す" } else { "既読にする" };
            if ui.button(read_label).clicked() {
                action = Some((d.id, !d.read));
            }
        });

        if *show_xml && !d.xml.is_empty() {
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
    let _ = sev_ink; // kept for colour parity with the web tags
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
