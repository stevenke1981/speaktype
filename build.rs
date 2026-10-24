fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint build failed");

    println!("cargo:rerun-if-changed=assets/speaktype.ico");
    // CUDA builds link nvcuda.dll (the NVIDIA driver). Delay-load it so the exe
    // still starts on PCs without an NVIDIA driver and simply runs on the CPU.
    if std::env::var_os("CARGO_FEATURE_CUDA").is_some()
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        for dll in ["nvcuda.dll", "cudart64_12.dll", "cublas64_12.dll", "cublasLt64_12.dll"] {
            println!("cargo:rustc-link-arg=/DELAYLOAD:{dll}");
        }
        println!("cargo:rustc-link-lib=delayimp");
    }
    // Vulkan builds: vulkan-1.dll ships with GPU drivers, but not on every PC.
    if std::env::var_os("CARGO_FEATURE_VULKAN").is_some()
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg=/DELAYLOAD:vulkan-1.dll");
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
