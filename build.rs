fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint build failed");

    println!("cargo:rerun-if-changed=assets/speaktype.ico");
    // CUDA builds link nvcuda.dll (the NVIDIA driver). Delay-load it so the exe
    // still starts on PCs without an NVIDIA driver and simply runs on the CPU.
    if std::env::var_os("CARGO_FEATURE_CUDA").is_some()
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg=/DELAYLOAD:nvcuda.dll");
        println!("cargo:rustc-link-lib=delayimp");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/speaktype.ico")
            .set("ProductName", "SpeakType")
            .set("FileDescription", "SpeakType - voice dictation")
            .set("LegalCopyright", "MIT License");
        res.compile().expect("failed to embed SpeakType Windows resources");
    }
}
