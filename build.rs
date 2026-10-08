fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=Cargo.toml");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let version =
            std::env::var("CARGO_PKG_VERSION").expect("Cargo must provide the package version");
        let mut resources = winres::WindowsResource::new();
        resources
            .set_icon("assets/icon.ico")
            .set("FileDescription", "WinBloat disk usage analyzer")
            .set("CompanyName", "TamKungZ_")
            .set("ProductName", "WinBloat")
            .set("InternalName", "WinBloat")
            .set("FileVersion", &version)
            .set("ProductVersion", &version)
            .set("LegalCopyright", "Copyright (c) 2026 TamKungZ_");
        resources
            .compile()
            .expect("failed to embed WinBloat Windows resources");
    }
}
