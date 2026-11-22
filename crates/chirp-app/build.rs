fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint UI");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icon.ico")
            .set("ProductName", "Chirp Wallpaper")
            .set("FileDescription", "Chirp Wallpaper")
            .set("CompanyName", "Chirp Wallpaper contributors")
            .set("LegalCopyright", "MIT License");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the icon: {e}");
        }
    }
    println!("cargo:rerun-if-changed=../../assets/icon.ico");
}
