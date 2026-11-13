//! Embeds the app icon (and version info) into the .exe so Windows
//! shortcuts and Explorer show it instead of the generic icon.

fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Rust Browser");
        res.set("FileDescription", "Rust Browser");
        res.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        res.set("FileVersion", env!("CARGO_PKG_VERSION"));
        if let Err(e) = res.compile() {
            // Don't fail the build if rc.exe isn't available.
            println!("cargo:warning=winres failed: {e}");
        }
    }
}
