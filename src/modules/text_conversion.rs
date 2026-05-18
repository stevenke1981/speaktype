use crate::modules::config::ChineseConversionMode;
use ferrous_opencc::config::BuiltinConfig;
use ferrous_opencc::OpenCC;

pub fn convert_chinese_text(text: &str, mode: ChineseConversionMode) -> Result<String, String> {
    match mode {
        ChineseConversionMode::Disabled => Ok(text.to_string()),
        ChineseConversionMode::TraditionalTaiwan => {
            let first_pass = convert_with_config(text, BuiltinConfig::S2twp)?;
            convert_with_config(&first_pass, BuiltinConfig::T2tw)
        }
        ChineseConversionMode::SimplifiedChina => convert_with_config(text, BuiltinConfig::Tw2sp),
    }
}

fn convert_with_config(text: &str, config: BuiltinConfig) -> Result<String, String> {
    OpenCC::from_config(config)
        .map_err(|err| err.to_string())
        .map(|converter| converter.convert(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_to_traditional_taiwan_terms() {
        let converted =
            convert_chinese_text("鼠标软件", ChineseConversionMode::TraditionalTaiwan).unwrap();

        assert!(converted.contains("滑鼠"));
        assert!(converted.contains("軟體"));
    }

    #[test]
    fn converts_to_simplified_china_terms() {
        let converted =
            convert_chinese_text("滑鼠軟體", ChineseConversionMode::SimplifiedChina).unwrap();

        assert!(converted.contains("鼠标"));
        assert!(converted.contains("软件"));
    }
}
