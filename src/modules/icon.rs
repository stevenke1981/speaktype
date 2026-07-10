const APP_ICON_PNG: &[u8] = include_bytes!("../../assets/speaktype-icon.png");

pub fn load_app_icon() -> Result<egui::IconData, String> {
    eframe::icon_data::from_png_bytes(APP_ICON_PNG).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_app_icon_is_valid_rgba() {
        let icon = load_app_icon().expect("embedded app icon should decode");

        assert_eq!((icon.width, icon.height), (512, 512));
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
        assert!(icon.rgba.chunks_exact(4).any(|pixel| pixel[3] == 0));
        assert!(icon.rgba.chunks_exact(4).any(|pixel| pixel[3] == 255));
    }
}
