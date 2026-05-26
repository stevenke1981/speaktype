// gui/mod.rs - egui 介面模組
// 職責：主視窗 UI、系統匣、狀態顯示

use crate::modules::scenario::Scenario;
use eframe::egui;

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
