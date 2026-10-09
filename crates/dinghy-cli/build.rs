//! Runs when the program is BUILT (not when it runs). On Windows it gives `dinghy.exe` its icon and
//! the information on the file's "Details" tab: product name, description and version (filled in
//! from `Cargo.toml` automatically).
//!
//! On other systems it does nothing.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/icons/windows-app.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("../../assets/icons/windows-app.ico");
    resource.set("ProductName", "BitDinghy");
    resource.set("FileDescription", "BitDinghy command line");
    resource.set("OriginalFilename", "dinghy.exe");
    resource.set(
        "LegalCopyright",
        "Copyright (c) 2026 BitDinghy contributors",
    );
    // A missing icon must not stop anyone from building the program, so this only warns.
    if let Err(error) = resource.compile() {
        println!("cargo:warning=couldn't add the Windows icon and version details: {error}");
    }
}
