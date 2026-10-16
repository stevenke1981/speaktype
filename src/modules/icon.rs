const APP_ICON_PNG: &[u8] = include_bytes!("../../assets/speaktype-icon.png");

/// Decoded RGBA icon (used for the tray icon on non-Windows platforms).
pub struct IconRgba {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub fn load_app_icon() -> Result<IconRgba, String> {
    let mut decoder = png::Decoder::new(APP_ICON_PNG);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|err| err.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|err| err.to_string())?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.as_chunks::<2>().0.iter().flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        other => return Err(format!("unsupported icon color type {other:?}")),
    };
    Ok(IconRgba { rgba, width: info.width, height: info.height })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_app_icon_is_valid_rgba() {
        let icon = load_app_icon().expect("embedded app icon should decode");

        assert_eq!((icon.width, icon.height), (512, 512));
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
        assert!(icon.rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] == 0));
        assert!(icon.rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] == 255));
    }
}
