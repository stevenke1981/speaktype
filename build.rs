fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint build failed");

    println!("cargo:rerun-if-changed=assets/speaktype.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/speaktype.ico")
            .set("ProductName", "SpeakType")
            .set("FileDescription", "SpeakType - voice dictation")
            .set("LegalCopyright", "MIT License");
        res.compile().expect("failed to embed SpeakType Windows resources");
    }
}
