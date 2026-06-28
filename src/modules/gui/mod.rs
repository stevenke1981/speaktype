// gui/mod.rs - egui 介面模組
// 職責：主視窗 UI、系統匣、狀態顯示、字型設定

pub mod theme;
pub mod views;

use crate::modules::config::{OutputRulesConfig, ScenarioOutputRules, VocabularyEntry};
use crate::modules::scenario::Scenario;
use eframe::egui;
use eframe::egui::{Color32, Rounding, Stroke, Vec2};
use std::fs;
use std::path::Path;
use theme::*;

pub struct GuiManager {
    pub show_history: bool,
}

impl GuiManager {
    pub fn new() -> Self {
        Self {
            show_history: false,
        }
    }

    /// 繪製裝置狀態（卡片式設計）
    pub fn draw_device_status(&self, ui: &mut egui::Ui, microphone: &str, gpu: &str, model: &str) {
        card_frame(ui).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("裝置狀態")
                        .size(14.0)
                        .color(text_primary()),
                );
            });
            ui.add_space(SPACE_SM);

            self.draw_status_row(ui, microphone, "麥克風");
            self.draw_status_row(ui, gpu, "GPU");
            self.draw_status_row(ui, model, "模型");
        });
    }

    fn draw_status_row(&self, ui: &mut egui::Ui, text: &str, label: &str) {
        ui.horizontal(|ui| {
            ui.add_space(SPACE_XS);
            let (color, dot) = status_dot(ui, text);
            ui.colored_label(color, dot);
            ui.label(
                egui::RichText::new(format!("{}：{}", label, text))
                    .color(text_primary()),
            );
        });
    }

    /// 繪製情境切換器（分段按鈕風格）
    pub fn draw_scenario_selector(
        &self,
        ui: &mut egui::Ui,
        current: Scenario,
        on_select: &mut dyn FnMut(Scenario),
    ) {
        section_frame(ui).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("情境")
                        .size(12.0)
                        .color(text_secondary()),
                );
                ui.add_space(SPACE_SM);
                for scenario in Scenario::all() {
                    let selected = current == scenario;
                    let text = egui::RichText::new(scenario.name())
                        .color(if selected { accent_color() } else { text_secondary() });
                    if ui
                        .selectable_label(selected, text)
                        .clicked()
                    {
                        on_select(scenario);
                    }
                }
            });
        });
    }

    /// 繪製錄音按鈕
    pub fn draw_record_button(&self, ui: &mut egui::Ui, recording: bool) -> bool {
        let button_text = if recording {
            "■ 停止錄音 (Ctrl+Shift+L)"
        } else {
            "● 開始錄音 (Ctrl+Shift+L)"
        };

        ui.button(button_text).clicked()
    }
}

impl Default for GuiManager {
    fn default() -> Self {
        Self::new()
    }
}

pub fn configure_cjk_fonts(ctx: &egui::Context) {
    let font_candidates = font_candidates_for_platform();
    let Some((font_name, font_bytes)) = font_candidates
        .iter()
        .find_map(|path| load_font_bytes(path).map(|bytes| (path.to_string(), bytes)))
    else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert(font_name.clone(), egui::FontData::from_owned(font_bytes));

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, font_name.clone());
    }

    ctx.set_fonts(fonts);
}

fn font_candidates_for_platform() -> &'static [&'static str] {
    #[cfg(target_os = "windows")]
    {
        &[
            r"C:\Windows\Fonts\NotoSansTC-VF.ttf",
            r"C:\Windows\Fonts\msjh.ttc",
            r"C:\Windows\Fonts\mingliu.ttc",
            r"C:\Windows\Fonts\simhei.ttf",
            r"C:\Windows\Fonts\simsun.ttc",
        ]
    }
    #[cfg(target_os = "linux")]
    {
        &[
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansTC-Regular.otf",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "/usr/share/fonts/wqy-microhei/wqy-microhei.ttc",
        ]
    }
    #[cfg(target_os = "macos")]
    {
        &[
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
            "/Library/Fonts/Arial Unicode.ttf",
        ]
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        &[]
    }
}

fn load_font_bytes(path: &str) -> Option<Vec<u8>> {
    fs::read(Path::new(path)).ok()
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;

    let bytes = bytes as f64;
    if bytes >= GB {
        format!("{:.2} GB", bytes / GB)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes / KB)
    } else {
        format!("{} B", bytes as u64)
    }
}

pub fn draw_vocabulary_settings(ui: &mut egui::Ui, entries: &mut Vec<VocabularyEntry>) -> bool {
    let mut changed = false;
    let mut remove_index = None;

    section_frame(ui).show(ui, |ui| {
        ui.label(
            egui::RichText::new("自訂詞庫 / 專有名詞")
                .size(13.0)
                .color(text_primary()),
        );
        ui.label(
            egui::RichText::new("source 是辨識可能出現的文字，replacement 是要輸出的正式寫法。")
                .size(11.0)
                .color(text_secondary()),
        );
        ui.add_space(SPACE_SM);

        egui::Grid::new("vocabulary_grid")
            .num_columns(4)
            .spacing([SPACE_SM, SPACE_SM])
            .striped(true)
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("source").color(text_secondary()),
                );
                ui.label(
                    egui::RichText::new("replacement").color(text_secondary()),
                );
                ui.label(
                    egui::RichText::new("保護").color(text_secondary()),
                );
                ui.label("");
                ui.end_row();

                for (index, entry) in entries.iter_mut().enumerate() {
                    changed |= ui.text_edit_singleline(&mut entry.source).changed();
                    changed |= ui.text_edit_singleline(&mut entry.replacement).changed();
                    changed |= ui.checkbox(&mut entry.protect, "").changed();
                    if ui.button("刪除").clicked() {
                        remove_index = Some(index);
                    }
                    ui.end_row();
                }
            });

        if let Some(index) = remove_index {
            entries.remove(index);
            changed = true;
        }

        ui.add_space(SPACE_SM);
        ui.horizontal(|ui| {
            if ui.button("＋ 新增詞彙").clicked() {
                entries.push(VocabularyEntry::blank());
                changed = true;
            }
            if ui.button("清除空白列").clicked() {
                entries.retain(|entry| !entry.is_blank());
                changed = true;
            }
        });
    });

    if entries.is_empty() {
        entries.push(VocabularyEntry::blank());
    }

    changed
}

pub fn draw_recording_overlay(
    ctx: &egui::Context,
    recording: bool,
    input_level: f32,
    elapsed_secs: f32,
    on_stop: &mut dyn FnMut(),
) {
    if !recording {
        return;
    }

    let area_id = egui::Id::new("recording_pill_overlay");
    let width = 320.0;
    let height = 52.0;
    let screen_size = ctx.screen_rect().size();
    let pos = egui::pos2((screen_size.x - width) / 2.0, 6.0);

    egui::Area::new(area_id)
        .fixed_pos(pos)
        .movable(false)
        .show(ctx, |ui| {
            let frame = egui::Frame::none()
                .fill(Color32::from_rgba_premultiplied(0, 0, 0, 200))
                .rounding(Rounding::same(RADIUS_XL))
                .stroke(Stroke::new(1.0, Color32::from_rgba_premultiplied(74, 108, 247, 80)))
                .shadow(egui::epaint::Shadow {
                    offset: Vec2::new(0.0, 8.0),
                    blur: 24.0,
                    spread: 0.0,
                    color: Color32::from_black_alpha(80),
                });
            frame.show(ui, |ui| {
                ui.set_min_size(Vec2::new(width, height));
                ui.horizontal_centered(|ui| {
                    ui.add_space(SPACE_MD);

                    // 脈動錄音紅點
                    let pulse = ((elapsed_secs * 6.0).sin() * 0.3 + 0.7) as u8;
                    ui.add(
                        egui::Button::new(
                            egui::RichText::new("●")
                                .size(18.0)
                                .color(Color32::from_rgba_premultiplied(255, 60, 60, pulse)),
                        )
                        .fill(Color32::TRANSPARENT)
                        .frame(false),
                    );

                    ui.add_space(SPACE_SM);

                    // 時間和標籤
                    ui.vertical_centered(|ui| {
                        ui.add_space(SPACE_XS);
                        ui.label(
                            egui::RichText::new("錄音中")
                                .size(13.0)
                                .color(text_primary()),
                        );
                        ui.label(
                            egui::RichText::new(format!("{:.1}s", elapsed_secs))
                                .size(10.0)
                                .color(text_secondary()),
                        );
                    });

                    ui.add_space(SPACE_SM);

                    // 音量條
                    let bar_width = 96.0;
                    let bar_height = 4.0;
                    let level = input_level.clamp(0.0, 1.0);
                    let bar_color = if level < 0.5 {
                        success_color()
                    } else if level < 0.8 {
                        warning_color()
                    } else {
                        error_color()
                    };
                    ui.add(
                        egui::ProgressBar::new(level)
                            .desired_width(bar_width)
                            .desired_height(bar_height)
                            .fill(bar_color)
                            .text(""),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("■")
                                        .size(14.0)
                                        .color(Color32::WHITE),
                                )
                                .fill(error_color())
                                .rounding(Rounding::same(RADIUS_SM))
                                .min_size(Vec2::new(32.0, 24.0)),
                            )
                            .clicked()
                        {
                            on_stop();
                        }
                    });
                    ui.add_space(SPACE_SM);
                });
            });
        });
}

pub fn draw_output_rules_settings(ui: &mut egui::Ui, rules: &mut OutputRulesConfig) -> bool {
    let mut changed = false;

    section_frame(ui).show(ui, |ui| {
        ui.label(
            egui::RichText::new("輸出規則模板")
                .size(13.0)
                .color(text_primary()),
        );
        ui.add_space(SPACE_SM);

        ui.collapsing(
            egui::RichText::new("聊天").color(text_primary()),
            |ui| {
                changed |= draw_rule_template(ui, &mut rules.chat);
            },
        );
        ui.collapsing(
            egui::RichText::new("寫作").color(text_primary()),
            |ui| {
                changed |= draw_rule_template(ui, &mut rules.writing);
            },
        );
        ui.collapsing(
            egui::RichText::new("程式碼").color(text_primary()),
            |ui| {
                changed |= draw_rule_template(ui, &mut rules.code);
            },
        );
    });

    changed
}

fn draw_rule_template(ui: &mut egui::Ui, rules: &mut ScenarioOutputRules) -> bool {
    let mut changed = false;

    changed |= ui
        .checkbox(&mut rules.auto_punctuation, "自動標點")
        .changed();
    changed |= ui
        .checkbox(&mut rules.format_paragraphs, "整理段落")
        .changed();
    changed |= ui
        .checkbox(&mut rules.remove_fillers, "去除語助詞")
        .changed();
    changed |= ui
        .checkbox(&mut rules.preserve_code_symbols, "保留英文符號")
        .changed();
    changed |= ui
        .checkbox(&mut rules.auto_line_breaks, "自動加換行")
        .changed();

    changed
}
