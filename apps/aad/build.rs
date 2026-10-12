//! Windows only: embed the app icon and version info into `aad.exe`. Elsewhere a no-op.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/aad.ico");
    println!("cargo:rerun-if-env-changed=AAD_REQUIRE_WINRES");
    println!("cargo:rerun-if-env-changed=AAD_BUILD_SHA");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    if std::path::Path::new("../../assets/app-icon/aad.ico").exists() {
        res.set_icon("../../assets/app-icon/aad.ico");
    }
    res.set("ProductName", "AAD")
        .set("FileDescription", "AAD computer-aided design")
        .set("CompanyName", "Learning Machines LLC")
        .set("LegalCopyright", "Copyright (c) 2026 ArtCraft Team and the AAD contributors. MIT OR Apache-2.0.")
        .set("OriginalFilename", "aad.exe")
        .set("InternalName", "aad");
    if let Err(e) = res.compile() {
        if std::env::var_os("AAD_REQUIRE_WINRES").is_some() {
            eprintln!("embedding Windows resources failed: {e}");
            std::process::exit(1);
        }
        println!("cargo:warning=aad.exe built without icon/version resources: {e}");
    }
}
