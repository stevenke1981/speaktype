// gui/theme.rs — SpeakType 多主題系統（設計師配色版）
// 使用方式：
//   1. 啟動時呼叫 init_theme(ctx, theme) 初始化
//   2. 切換主題時呼叫 switch_theme(ctx, theme)
//   3. 各處使用 accent_color(), text_primary() 等函數存取當前色彩

use eframe::egui;
use egui::{Color32, Rounding, Stroke, Style, Vec2, Visuals, Margin};
use std::sync::Mutex;

// ===================== Theme Variant =====================

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SpeakTypeTheme {
    /// 高級、精品、耐看（淺色）
    QuietLuxury,
    /// 奶油黃、清爽、少女但不幼稚（淺色）
    ButterCreamMinimal,
    /// 時尚科技、精品後台、穩重（深色）
    TransformativeTealAtelier,
    /// 雜誌封面、黑色高級感（深色）
    EditorialNoirSoftRed,
    /// 牛仔、日系選物、乾淨工具感（淺色）
    DenimAtelier,
    /// 芭蕾粉、柔和、女性向 UI（淺色）
    BalletcoreRose,
    /// 潮流科技、機能風、亮但不螢光（深色）
    MutedLimeTechwear,
    /// 紫色精品、晚宴、時尚編輯感（淺色）
    PlumCouture,
}

impl SpeakTypeTheme {
    pub fn all() -> &'static [Self] {
        &[
            Self::QuietLuxury,
            Self::ButterCreamMinimal,
            Self::TransformativeTealAtelier,
            Self::EditorialNoirSoftRed,
            Self::DenimAtelier,
            Self::BalletcoreRose,
            Self::MutedLimeTechwear,
            Self::PlumCouture,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::QuietLuxury => "Quiet Luxury 精品",
            Self::ButterCreamMinimal => "Butter Cream 奶油",
            Self::TransformativeTealAtelier => "Teal Atelier 深青",
            Self::EditorialNoirSoftRed => "Editorial Noir 墨紅",
            Self::DenimAtelier => "Denim Atelier 丹寧",
            Self::BalletcoreRose => "Balletcore 芭蕾粉",
            Self::MutedLimeTechwear => "Muted Lime 萊姆綠",
            Self::PlumCouture => "Plum Couture 紫韻",
        }
    }

    pub fn is_dark(&self) -> bool {
        matches!(
            self,
            Self::TransformativeTealAtelier
                | Self::EditorialNoirSoftRed
                | Self::MutedLimeTechwear
        )
    }
}

// ===================== Global Current Theme =====================

static CURRENT_THEME: Mutex<SpeakTypeTheme> = Mutex::new(SpeakTypeTheme::QuietLuxury);

/// 取得目前主題
pub fn current_theme() -> SpeakTypeTheme {
    *CURRENT_THEME.lock().unwrap()
}

/// 設定目前主題（不套用樣式）
fn set_current_theme(theme: SpeakTypeTheme) {
    *CURRENT_THEME.lock().unwrap() = theme;
}

// ===================== 色彩輔助 =====================

fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    Color32::from_rgba_premultiplied(
        (a.r() as f32 * (1.0 - t) + b.r() as f32 * t) as u8,
        (a.g() as f32 * (1.0 - t) + b.g() as f32 * t) as u8,
        (a.b() as f32 * (1.0 - t) + b.b() as f32 * t) as u8,
        255,
    )
}

// ===================== Theme-specific Base Colors =====================

/// Background — 視窗底色
fn bg(theme: SpeakTypeTheme) -> Color32 {
    match theme {
        SpeakTypeTheme::QuietLuxury => Color32::from_rgb(247, 243, 234),            // #F7F3EA
        SpeakTypeTheme::ButterCreamMinimal => Color32::from_rgb(255, 249, 232),     // #FFF9E8
        SpeakTypeTheme::TransformativeTealAtelier => Color32::from_rgb(16, 42, 46), // #102A2E
        SpeakTypeTheme::EditorialNoirSoftRed => Color32::from_rgb(17, 17, 19),      // #111113
        SpeakTypeTheme::DenimAtelier => Color32::from_rgb(242, 245, 247),           // #F2F5F7
        SpeakTypeTheme::BalletcoreRose => Color32::from_rgb(255, 244, 246),         // #FFF4F6
        SpeakTypeTheme::MutedLimeTechwear => Color32::from_rgb(21, 24, 23),         // #151817
        SpeakTypeTheme::PlumCouture => Color32::from_rgb(247, 243, 251),            // #F7F3FB
    }
}

/// Panel — 面板區塊底色
fn panel_bg(theme: SpeakTypeTheme) -> Color32 {
    match theme {
        SpeakTypeTheme::QuietLuxury => Color32::from_rgb(238, 229, 214),            // #EEE5D6
        SpeakTypeTheme::ButterCreamMinimal => Color32::from_rgb(246, 232, 190),     // #F6E8BE
        SpeakTypeTheme::TransformativeTealAtelier => Color32::from_rgb(22, 56, 61), // #16383D
        SpeakTypeTheme::EditorialNoirSoftRed => Color32::from_rgb(27, 27, 31),      // #1B1B1F
        SpeakTypeTheme::DenimAtelier => Color32::from_rgb(221, 232, 240),           // #DDE8F0
        SpeakTypeTheme::BalletcoreRose => Color32::from_rgb(244, 221, 228),         // #F4DDE4
        SpeakTypeTheme::MutedLimeTechwear => Color32::from_rgb(32, 37, 34),         // #202522
        SpeakTypeTheme::PlumCouture => Color32::from_rgb(233, 222, 243),            // #E9DEF3
    }
}

/// Text — 主要文字色
fn text_color(theme: SpeakTypeTheme) -> Color32 {
    match theme {
        SpeakTypeTheme::QuietLuxury => Color32::from_rgb(42, 38, 32),               // #2A2620
        SpeakTypeTheme::ButterCreamMinimal => Color32::from_rgb(46, 42, 31),        // #2E2A1F
        SpeakTypeTheme::TransformativeTealAtelier => Color32::from_rgb(234, 244, 242), // #EAF4F2
        SpeakTypeTheme::EditorialNoirSoftRed => Color32::from_rgb(241, 238, 232),   // #F1EEE8
        SpeakTypeTheme::DenimAtelier => Color32::from_rgb(23, 36, 51),              // #172433
        SpeakTypeTheme::BalletcoreRose => Color32::from_rgb(56, 40, 46),            // #38282E
        SpeakTypeTheme::MutedLimeTechwear => Color32::from_rgb(231, 238, 233),      // #E7EEE9
        SpeakTypeTheme::PlumCouture => Color32::from_rgb(39, 29, 49),               // #271D31
    }
}

/// Primary — 主要強調色（按鈕、連結、啟動狀態）
fn primary(theme: SpeakTypeTheme) -> Color32 {
    match theme {
        SpeakTypeTheme::QuietLuxury => Color32::from_rgb(139, 111, 71),             // #8B6F47
        SpeakTypeTheme::ButterCreamMinimal => Color32::from_rgb(138, 100, 24),      // #8A6418
        SpeakTypeTheme::TransformativeTealAtelier => Color32::from_rgb(79, 183, 176), // #4FB7B0
        SpeakTypeTheme::EditorialNoirSoftRed => Color32::from_rgb(201, 75, 75),     // #C94B4B
        SpeakTypeTheme::DenimAtelier => Color32::from_rgb(60, 111, 149),            // #3C6F95
        SpeakTypeTheme::BalletcoreRose => Color32::from_rgb(155, 79, 92),           // #9B4F5C
        SpeakTypeTheme::MutedLimeTechwear => Color32::from_rgb(167, 201, 87),       // #A7C957
        SpeakTypeTheme::PlumCouture => Color32::from_rgb(114, 83, 155),              // #72539B
    }
}

/// Hover — 懸停/細緻背景色
fn hover_bg(theme: SpeakTypeTheme) -> Color32 {
    match theme {
        SpeakTypeTheme::QuietLuxury => Color32::from_rgb(229, 216, 194),            // #E5D8C2
        SpeakTypeTheme::ButterCreamMinimal => Color32::from_rgb(240, 223, 165),     // #F0DFA5
        SpeakTypeTheme::TransformativeTealAtelier => Color32::from_rgb(31, 75, 81), // #1F4B51
        SpeakTypeTheme::EditorialNoirSoftRed => Color32::from_rgb(42, 42, 48),      // #2A2A30
        SpeakTypeTheme::DenimAtelier => Color32::from_rgb(201, 218, 229),           // #C9DAE5
        SpeakTypeTheme::BalletcoreRose => Color32::from_rgb(236, 208, 216),         // #ECD0D8
        SpeakTypeTheme::MutedLimeTechwear => Color32::from_rgb(41, 48, 43),         // #29302B
        SpeakTypeTheme::PlumCouture => Color32::from_rgb(220, 206, 234),            // #DCCEEA
    }
}

/// Selection — 選取高亮底色
fn selection_bg(theme: SpeakTypeTheme) -> Color32 {
    match theme {
        SpeakTypeTheme::QuietLuxury => Color32::from_rgb(209, 181, 138),            // #D1B58A
        SpeakTypeTheme::ButterCreamMinimal => Color32::from_rgb(229, 196, 107),     // #E5C46B
        SpeakTypeTheme::TransformativeTealAtelier => Color32::from_rgb(40, 107, 112), // #286B70
        SpeakTypeTheme::EditorialNoirSoftRed => Color32::from_rgb(91, 47, 51),      // #5B2F33
        SpeakTypeTheme::DenimAtelier => Color32::from_rgb(143, 177, 201),           // #8FB1C9
        SpeakTypeTheme::BalletcoreRose => Color32::from_rgb(223, 174, 187),         // #DFAEBB
        SpeakTypeTheme::MutedLimeTechwear => Color32::from_rgb(64, 82, 57),         // #405239
        SpeakTypeTheme::PlumCouture => Color32::from_rgb(183, 162, 210),            // #B7A2D2
    }
}

/// Accent — 次要強調色（裝飾用）
fn secondary_accent(theme: SpeakTypeTheme) -> Color32 {
    match theme {
        SpeakTypeTheme::QuietLuxury => Color32::from_rgb(94, 107, 86),              // #5E6B56
        SpeakTypeTheme::ButterCreamMinimal => Color32::from_rgb(110, 127, 95),      // #6E7F5F
        SpeakTypeTheme::TransformativeTealAtelier => Color32::from_rgb(215, 197, 154), // #D7C59A
        SpeakTypeTheme::EditorialNoirSoftRed => Color32::from_rgb(226, 201, 166),   // #E2C9A6
        SpeakTypeTheme::DenimAtelier => Color32::from_rgb(169, 132, 103),           // #A98467
        SpeakTypeTheme::BalletcoreRose => Color32::from_rgb(138, 124, 104),         // #8A7C68
        SpeakTypeTheme::MutedLimeTechwear => Color32::from_rgb(214, 180, 108),      // #D6B46C
        SpeakTypeTheme::PlumCouture => Color32::from_rgb(182, 126, 111),            // #B67E6F
    }
}

// ===================== 衍生色彩 =====================

fn card_bg_color(theme: SpeakTypeTheme) -> Color32 {
    blend(panel_bg(theme), bg(theme), 0.4)
}

fn border_color_fn(theme: SpeakTypeTheme) -> Color32 {
    blend(text_color(theme), panel_bg(theme), 0.88)
}

fn text_secondary_color_fn(theme: SpeakTypeTheme) -> Color32 {
    blend(text_color(theme), bg(theme), 0.45)
}

fn text_dim_color_fn(theme: SpeakTypeTheme) -> Color32 {
    blend(text_color(theme), bg(theme), 0.68)
}

fn primary_hover(theme: SpeakTypeTheme) -> Color32 {
    let c = primary(theme);
    let b = if theme.is_dark() { Color32::BLACK } else { Color32::WHITE };
    blend(c, b, 0.15)
}

// ===================== 狀態色（通用，各主題微調） =====================

fn success(theme: SpeakTypeTheme) -> Color32 {
    if theme.is_dark() { Color32::from_rgb(68, 207, 110) } else { Color32::from_rgb(34, 154, 74) }
}
fn warning(theme: SpeakTypeTheme) -> Color32 {
    if theme.is_dark() { Color32::from_rgb(245, 178, 52) } else { Color32::from_rgb(194, 138, 24) }
}
fn error(theme: SpeakTypeTheme) -> Color32 {
    if theme.is_dark() { Color32::from_rgb(239, 88, 88) } else { Color32::from_rgb(210, 48, 48) }
}
fn info(theme: SpeakTypeTheme) -> Color32 {
    if theme.is_dark() { Color32::from_rgb(79, 150, 245) } else { Color32::from_rgb(47, 99, 215) }
}

// ===================== 公開色彩存取（基於目前主題） =====================

pub fn accent_color() -> Color32 { primary(current_theme()) }
pub fn accent_hover_color() -> Color32 { primary_hover(current_theme()) }
pub fn secondary_accent_color() -> Color32 { secondary_accent(current_theme()) }
pub fn success_color() -> Color32 { success(current_theme()) }
pub fn warning_color() -> Color32 { warning(current_theme()) }
pub fn error_color() -> Color32 { error(current_theme()) }
pub fn info_color() -> Color32 { info(current_theme()) }
pub fn text_primary() -> Color32 { text_color(current_theme()) }
pub fn text_secondary() -> Color32 { text_secondary_color_fn(current_theme()) }
pub fn text_dim() -> Color32 { text_dim_color_fn(current_theme()) }
pub fn bg_faint() -> Color32 { hover_bg(current_theme()) }
pub fn bg_card() -> Color32 { card_bg_color(current_theme()) }
pub fn border() -> Color32 { border_color_fn(current_theme()) }
pub fn selection_color() -> Color32 { selection_bg(current_theme()) }

// ===================== 間距與圓角 Token（所有主題共用） =====================

pub const SPACE_XS: f32 = 4.0;
pub const SPACE_SM: f32 = 8.0;
pub const SPACE_MD: f32 = 16.0;
pub const SPACE_LG: f32 = 24.0;
pub const SPACE_XL: f32 = 32.0;

pub const RADIUS_SM: f32 = 4.0;
pub const RADIUS_MD: f32 = 6.0;
pub const RADIUS_LG: f32 = 8.0;
pub const RADIUS_XL: f32 = 12.0;

// ===================== 主題套用 =====================

/// 初始化並套用主題（啟動時呼叫）
pub fn init_theme(ctx: &egui::Context, theme: SpeakTypeTheme) {
    set_current_theme(theme);
    apply_theme(ctx, theme);
}

/// 切換主題並套用（執行中切換用）
pub fn switch_theme(ctx: &egui::Context, theme: SpeakTypeTheme) {
    set_current_theme(theme);
    apply_theme(ctx, theme);
}

fn apply_theme(ctx: &egui::Context, theme: SpeakTypeTheme) {
    let dark = theme.is_dark();
    let a = primary(theme);
    let a_hover = primary_hover(theme);
    let sel = selection_bg(theme);

    let wf = bg(theme);
    let pf = panel_bg(theme);
    let fb = hover_bg(theme);
    let cf = card_bg_color(theme);
    let bd = border_color_fn(theme);
    let tx = text_color(theme);
    let tx2 = text_secondary_color_fn(theme);

    let mut style = Style {
        visuals: Visuals {
            dark_mode: dark,

            window_fill: wf,
            panel_fill: pf,
            faint_bg_color: fb,
            override_text_color: Some(tx),
            extreme_bg_color: wf,

            window_stroke: Stroke::new(1.0, bd),
            window_shadow: egui::epaint::Shadow {
                offset: Vec2::new(0.0, 8.0),
                blur: 24.0,
                spread: 0.0,
                color: if dark { Color32::from_black_alpha(80) } else { Color32::from_black_alpha(16) },
            },

            button_frame: true,
            collapsing_header_frame: true,

            widgets: egui::style::Widgets {
                noninteractive: egui::style::WidgetVisuals {
                    bg_fill: fb,
                    weak_bg_fill: fb,
                    fg_stroke: Stroke::new(1.0, tx2),
                    bg_stroke: Stroke::new(1.0, bd),
                    rounding: Rounding::same(RADIUS_SM),
                    expansion: 0.0,
                },
                inactive: egui::style::WidgetVisuals {
                    bg_fill: cf,
                    weak_bg_fill: cf,
                    fg_stroke: Stroke::new(1.5, tx),
                    bg_stroke: Stroke::new(1.0, bd),
                    rounding: Rounding::same(RADIUS_MD),
                    expansion: 0.0,
                },
                hovered: egui::style::WidgetVisuals {
                    bg_fill: fb,
                    weak_bg_fill: fb,
                    fg_stroke: Stroke::new(1.5, tx),
                    bg_stroke: Stroke::new(1.0, a),
                    rounding: Rounding::same(RADIUS_MD),
                    expansion: 1.0,
                },
                active: egui::style::WidgetVisuals {
                    bg_fill: a,
                    weak_bg_fill: a,
                    fg_stroke: Stroke::new(2.0, Color32::WHITE),
                    bg_stroke: Stroke::new(1.0, a_hover),
                    rounding: Rounding::same(RADIUS_MD),
                    expansion: 0.0,
                },
                open: egui::style::WidgetVisuals {
                    bg_fill: fb,
                    weak_bg_fill: fb,
                    fg_stroke: Stroke::new(2.0, a),
                    bg_stroke: Stroke::new(1.0, a),
                    rounding: Rounding::same(RADIUS_MD),
                    expansion: 0.0,
                },
            },

            selection: egui::style::Selection {
                bg_fill: sel,
                stroke: Stroke::new(1.0, a),
            },

            hyperlink_color: a,

            ..Default::default()
        },
        ..Style::default()
    };

    style.spacing.item_spacing = Vec2::new(SPACE_SM, SPACE_SM);
    style.spacing.button_padding = Vec2::new(SPACE_MD, SPACE_SM);
    style.spacing.indent = SPACE_LG;
    style.spacing.window_margin = Margin::symmetric(SPACE_MD, SPACE_MD);
    style.spacing.interact_size = Vec2::new(40.0, 24.0);
    style.animation_time = 0.2;

    ctx.set_style(style);
}

// ===================== 狀態指示燈 =====================

/// 回傳 (色彩, 符號) 依據狀態文字判斷
pub fn status_dot<'a>(_ui: &mut egui::Ui, state: &'a str) -> (Color32, &'a str) {
    let theme = current_theme();
    let color = if state.contains("就緒")
        || state.contains("已載入")
        || state.contains("已準備")
        || state.contains("完成")
    {
        success(theme)
    } else if state.contains("未找到") || state.contains("警告") {
        warning(theme)
    } else if state.contains("失敗") || state.contains("錯誤") {
        error(theme)
    } else if state.contains("錄音") || state.contains("忙碌") {
        info(theme)
    } else {
        text_secondary_color_fn(theme)
    };
    (color, "●")
}

// ===================== Frame 工廠 =====================

/// 卡片 Frame（預留 _ui 供未來 per-ui 主題偵測）
pub fn card_frame(_ui: &egui::Ui) -> egui::Frame {
    let theme = current_theme();
    egui::Frame::none()
        .fill(card_bg_color(theme))
        .stroke(Stroke::new(1.0, border_color_fn(theme)))
        .rounding(Rounding::same(RADIUS_LG))
        .inner_margin(Margin::symmetric(SPACE_MD, SPACE_MD))
}

/// 子區段 Frame
pub fn section_frame(_ui: &egui::Ui) -> egui::Frame {
    let theme = current_theme();
    egui::Frame::none()
        .fill(hover_bg(theme))
        .stroke(Stroke::new(1.0, border_color_fn(theme)))
        .rounding(Rounding::same(RADIUS_SM))
        .inner_margin(Margin::symmetric(SPACE_SM, SPACE_SM))
}

/// 下載進度條用 Frame（背景）
pub fn progress_frame(_ui: &egui::Ui) -> egui::Frame {
    let theme = current_theme();
    egui::Frame::none()
        .fill(if theme.is_dark() {
            blend(bg(theme), panel_bg(theme), 0.3)
        } else {
            Color32::from_rgb(255, 255, 255)
        })
        .rounding(Rounding::same(RADIUS_SM))
        .inner_margin(Margin::symmetric(SPACE_SM, SPACE_SM))
}
