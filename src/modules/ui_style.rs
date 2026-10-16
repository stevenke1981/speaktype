//! UI-toolkit–independent styling helpers: colour themes, CJK UI font choice, formatting.

/// One colour theme: (background, panel, text, accent, hover) as RGB, plus dark flag.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub id: &'static str,
    pub label: &'static str,
    pub bg: [u8; 3],
    pub panel: [u8; 3],
    pub text: [u8; 3],
    pub accent: [u8; 3],
    pub hover: [u8; 3],
    pub dark: bool,
}

pub const THEMES: [Theme; 8] = [
    Theme { id: "quiet-luxury", label: "Quiet Luxury 精品", bg: [247, 243, 234], panel: [238, 229, 214], text: [42, 38, 32], accent: [139, 111, 71], hover: [229, 216, 194], dark: false },
    Theme { id: "butter-cream", label: "Butter Cream 奶油", bg: [255, 249, 232], panel: [246, 232, 190], text: [46, 42, 31], accent: [138, 100, 24], hover: [240, 223, 165], dark: false },
    Theme { id: "teal-atelier", label: "Teal Atelier 深青", bg: [16, 42, 46], panel: [22, 56, 61], text: [234, 244, 242], accent: [79, 183, 176], hover: [31, 75, 81], dark: true },
    Theme { id: "editorial-noir", label: "Editorial Noir 墨紅", bg: [17, 17, 19], panel: [27, 27, 31], text: [241, 238, 232], accent: [201, 75, 75], hover: [42, 42, 48], dark: true },
    Theme { id: "denim-atelier", label: "Denim Atelier 丹寧", bg: [242, 245, 247], panel: [221, 232, 240], text: [23, 36, 51], accent: [60, 111, 149], hover: [201, 218, 229], dark: false },
    Theme { id: "balletcore-rose", label: "Balletcore 芭蕾粉", bg: [255, 244, 246], panel: [244, 221, 228], text: [56, 40, 46], accent: [155, 79, 92], hover: [236, 208, 216], dark: false },
    Theme { id: "muted-lime", label: "Muted Lime 萊姆綠", bg: [21, 24, 23], panel: [32, 37, 34], text: [231, 238, 233], accent: [167, 201, 87], hover: [41, 48, 43], dark: true },
    Theme { id: "plum-couture", label: "Plum Couture 紫韻", bg: [247, 243, 251], panel: [233, 222, 243], text: [39, 29, 49], accent: [114, 83, 155], hover: [220, 206, 234], dark: false },
];

pub fn theme_index(id: Option<&str>) -> usize {
    id.and_then(|id| THEMES.iter().position(|t| t.id == id)).unwrap_or(0)
}

/// UI font family. On Windows the default fallback can pick a Japanese/Simplified
/// face that lacks Traditional characters, so name an installed CJK UI font
/// (Microsoft JhengHei UI first). Elsewhere the platform default is fine.
pub fn ui_font_family() -> &'static str {
    ui_font_family_from(|p| std::path::Path::new(p).exists(), cfg!(windows))
}

pub fn ui_font_family_from(exists: impl Fn(&str) -> bool, windows: bool) -> &'static str {
    if !windows {
        return "";
    }
    const CANDIDATES: [(&str, &str); 4] = [
        (r"C:\Windows\Fonts\msjh.ttc", "Microsoft JhengHei UI"),
        (r"C:\Windows\Fonts\msyh.ttc", "Microsoft YaHei UI"),
        (r"C:\Windows\Fonts\mingliu.ttc", "PMingLiU"),
        (r"C:\Windows\Fonts\simsun.ttc", "SimSun"),
    ];
    CANDIDATES.iter().find(|(file, _)| exists(file)).map(|(_, family)| *family).unwrap_or("")
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_prefers_jhenghei_then_yahei() {
        assert_eq!(ui_font_family_from(|_| true, true), "Microsoft JhengHei UI");
        assert_eq!(ui_font_family_from(|p| p.ends_with("msyh.ttc"), true), "Microsoft YaHei UI");
        assert_eq!(ui_font_family_from(|_| false, true), "");
        assert_eq!(ui_font_family_from(|_| true, false), "");
    }

    #[test]
    fn theme_lookup_falls_back_to_first() {
        assert_eq!(theme_index(Some("muted-lime")), 6);
        assert_eq!(theme_index(Some("nope")), 0);
        assert_eq!(theme_index(None), 0);
    }

    #[test]
    fn bytes_are_human_readable() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }
}
