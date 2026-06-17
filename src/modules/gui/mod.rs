// gui/mod.rs - egui 介面模組
// 職責：主視窗 UI、系統匣、狀態顯示、字型設定

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
