fn main() {
    println!("cargo:rerun-if-changed=assets/speaktype.ico");

    #[cfg(windows)]
    winresource::WindowsResource::new()
        .set_icon("assets/speaktype.ico")
        .compile()
        .expect("failed to embed SpeakType Windows resources");
}
