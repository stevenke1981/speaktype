use crate::modules::gui;
use crate::modules::gui::theme::*;
use crate::modules::history::HistoryManager;
use crate::modules::recordings::RecordingFile;
use eframe::egui;
use eframe::egui::Rounding;
use std::path::Path;

pub fn draw_history_window(
    ctx: &egui::Context,
    open: &mut bool,
    history: &HistoryManager,
    on_clear: &mut dyn FnMut(),
) {
    egui::Window::new("辨識紀錄")
        .open(open)
        .resizable(true)
        .default_width(520.0)
        .show(ctx, |ui| {
            if history.records().is_empty() {
                ui.label(
                    egui::RichText::new("尚無紀錄").color(text_secondary()),
                );
                return;
            }

            if let Some(path) = HistoryManager::history_path() {
                ui.label(
                    egui::RichText::new(format!("紀錄檔：{}", path.display()))
                        .size(11.0)
                        .color(text_secondary()),
                );
                ui.separator();
            }

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for record in history.records() {
                        card_frame(ui).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(
                                        record.timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
                                    )
                                    .size(11.0)
                                    .color(text_secondary()),
                                );
                                ui.label(
                                    egui::RichText::new(format!("[{}]", record.scenario))
                                        .size(11.0)
                                        .color(accent_color()),
                                );
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{:.1} 秒", record.duration_sec))
                                            .size(11.0)
                                            .color(text_secondary()),
                                    );
                                });
                            });
                            ui.add_space(SPACE_XS);
                            let mut text = record.text.clone();
                            ui.add(
                                egui::TextEdit::multiline(&mut text)
                                    .desired_width(f32::INFINITY)
                                    .interactive(false)
                                    .text_color(text_primary()),
                            );
                        });
                        ui.add_space(SPACE_SM);
                    }
                });

            ui.add_space(SPACE_SM);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("清除紀錄").clicked() {
                    on_clear();
                }
            });
        });
}

pub fn draw_error_window(
    ctx: &egui::Context,
    open: &mut bool,
    errors: &[String],
    log_path: &Path,
    on_clear: &mut dyn FnMut(),
) {
    egui::Window::new("錯誤紀錄")
        .open(open)
        .resizable(true)
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(format!("Log 檔案：{}", log_path.display()))
                    .size(11.0)
                    .color(text_secondary()),
            );
            ui.separator();
            if errors.is_empty() {
                ui.label(
                    egui::RichText::new("目前沒有錯誤紀錄").color(text_secondary()),
                );
            } else {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for error in errors {
                            card_frame(ui).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.colored_label(error_color(), "⚠");
                                    ui.add_space(SPACE_SM);
                                    ui.label(
                                        egui::RichText::new(error)
                                            .color(text_primary())
                                            .size(12.0),
                                    );
                                });
                            });
                            ui.add_space(SPACE_SM);
                        }
                    });
                ui.add_space(SPACE_SM);
                ui.horizontal(|ui| {
                    if ui.button("清除畫面紀錄").clicked() {
                        on_clear();
                    }
                });
            }
        });
}

pub fn draw_model_download_status(
    ui: &mut egui::Ui,
    progress: Option<&crate::modules::engine::ModelDownloadProgress>,
    on_cancel: &mut dyn FnMut(),
    on_retry: &mut dyn FnMut(),
) {
    let Some(progress) = progress else { return };

    let fraction = progress
        .total_bytes
        .map(|total| progress.downloaded_bytes as f32 / total.max(1) as f32)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);

    card_frame(ui).show(ui, |ui| {
        ui.label(
            egui::RichText::new("下載中")
                .size(13.0)
                .color(text_primary()),
        );
        ui.add_space(SPACE_SM);
        ui.add(
            egui::ProgressBar::new(fraction)
                .show_percentage()
                .fill(accent_color())
                .desired_height(6.0),
        );
        ui.add_space(SPACE_XS);

        let total = progress
            .total_bytes
            .map(gui::format_bytes)
            .unwrap_or_else(|| "未知大小".to_string());
        ui.label(
            egui::RichText::new(format!(
                "{} / {}　{}/s",
                gui::format_bytes(progress.downloaded_bytes),
                total,
                gui::format_bytes(progress.speed_bytes_per_sec as u64)
            ))
            .size(11.0)
            .color(text_secondary()),
        );
        ui.label(
            egui::RichText::new(format!("來源：{}", progress.url))
                .size(10.0)
                .color(text_dim()),
        );
        ui.add_space(SPACE_SM);
        ui.horizontal(|ui| {
            if ui.button("取消下載").clicked() {
                on_cancel();
            }
            if ui.button("重試下載").clicked() {
                on_retry();
            }
        });
    });
}

pub fn draw_recordings_list(
    ui: &mut egui::Ui,
    files: &[RecordingFile],
    on_play: &mut dyn FnMut(&Path),
    on_retry: &mut dyn FnMut(&Path),
    on_delete: &mut dyn FnMut(&Path),
) {
    if files.is_empty() {
        ui.label(
            egui::RichText::new("沒有符合條件的錄音檔").color(text_secondary()),
        );
        return;
    }

    let total_size = files.iter().map(|file| file.size_bytes).sum::<u64>();
    ui.label(
        egui::RichText::new(format!("共 {} 筆，{}", files.len(), gui::format_bytes(total_size)))
            .size(11.0)
            .color(text_secondary()),
    );
    ui.add_space(SPACE_SM);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for file in files {
                card_frame(ui).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(
                                file.modified.format("%Y-%m-%d %H:%M:%S").to_string(),
                            )
                            .size(11.0)
                            .color(text_secondary()),
                        );
                        ui.monospace(&file.file_name);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(gui::format_bytes(file.size_bytes))
                                    .color(text_secondary()),
                            );
                        });
                    });
                    ui.add_space(SPACE_SM);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("播放").color(text_primary()),
                                )
                                .fill(bg_faint())
                                .rounding(Rounding::same(RADIUS_SM)),
                            )
                            .clicked()
                        {
                            on_play(&file.path);
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("重新轉錄").color(text_primary()),
                                )
                                .fill(bg_faint())
                                .rounding(Rounding::same(RADIUS_SM)),
                            )
                            .clicked()
                        {
                            on_retry(&file.path);
                        }
                        if ui
                            .add(
                                egui::Button::new(egui::RichText::new("刪除").color(error_color()))
                                    .fill(bg_faint())
                                    .rounding(Rounding::same(RADIUS_SM)),
                            )
                            .clicked()
                        {
                            on_delete(&file.path);
                        }
                    });
                });
                ui.add_space(SPACE_SM);
            }
        });
}
