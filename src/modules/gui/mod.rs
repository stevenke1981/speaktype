// gui/mod.rs - egui 介面模組
// 職責：主視窗 UI、系統匣、狀態顯示、字型設定

use crate::modules::config::{OutputRulesConfig, ScenarioOutputRules, VocabularyEntry};
use crate::modules::scenario::Scenario;
use eframe::egui;
use std::fs;
use std::path::Path;

pub struct GuiManager {
    pub show_history: bool,
}

impl GuiManager {
    pub fn new() -> Self {
        Self {
            show_history: false,
        }
    }

    /// 繪製裝置狀態
    pub fn draw_device_status(&self, ui: &mut egui::Ui, microphone: &str, gpu: &str, model: &str) {
        ui.label("裝置狀態");
        ui.add_space(6.0);

        self.draw_status_row(ui, microphone, "麥克風");
        self.draw_status_row(ui, gpu, "GPU");
        self.draw_status_row(ui, model, "模型");
    }

    fn draw_status_row(&self, ui: &mut egui::Ui, text: &str, label: &str) {
        ui.horizontal(|ui| {
            let color =
                if text.contains("就緒") || text.contains("已載入") || text.contains("已準備")
                {
                    egui::Color32::from_rgb(0, 200, 100)
                } else if text.contains("未找到") {
                    egui::Color32::from_rgb(255, 180, 0)
                } else {
                    egui::Color32::RED
                };
            ui.colored_label(color, "●");
            ui.label(format!("{}：{}", label, text));
        });
    }

    /// 繪製情境切換器
    pub fn draw_scenario_selector(
        &self,
        ui: &mut egui::Ui,
        current: Scenario,
        on_select: &mut dyn FnMut(Scenario),
    ) {
        ui.label("情境：");
        ui.horizontal(|ui| {
            for scenario in Scenario::all() {
                let selected = current == scenario;
                if ui.selectable_label(selected, scenario.name()).clicked() {
                    on_select(scenario);
                }
            }
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
    let font_candidates = [
        r"C:\Windows\Fonts\NotoSansTC-VF.ttf",
        r"C:\Windows\Fonts\msjh.ttc",
        r"C:\Windows\Fonts\mingliu.ttc",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsun.ttc",
    ];

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

    ui.label("自訂詞庫 / 專有名詞");
    ui.label("source 是辨識可能出現的文字，replacement 是要輸出的正式寫法。");

    egui::Grid::new("vocabulary_grid")
        .num_columns(4)
        .spacing([8.0, 6.0])
        .striped(true)
        .show(ui, |ui| {
            ui.label("source");
            ui.label("replacement");
            ui.label("保護");
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

    ui.horizontal(|ui| {
        if ui.button("新增詞彙").clicked() {
            entries.push(VocabularyEntry::blank());
            changed = true;
        }
        if ui.button("清除空白列").clicked() {
            entries.retain(|entry| !entry.is_blank());
            changed = true;
        }
    });

    if entries.is_empty() {
        entries.push(VocabularyEntry::blank());
    }

    changed
}

pub fn draw_output_rules_settings(ui: &mut egui::Ui, rules: &mut OutputRulesConfig) -> bool {
    let mut changed = false;

    ui.label("輸出規則模板");
    ui.collapsing("聊天", |ui| {
        changed |= draw_rule_template(ui, &mut rules.chat);
    });
    ui.collapsing("寫作", |ui| {
        changed |= draw_rule_template(ui, &mut rules.writing);
    });
    ui.collapsing("程式碼", |ui| {
        changed |= draw_rule_template(ui, &mut rules.code);
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
