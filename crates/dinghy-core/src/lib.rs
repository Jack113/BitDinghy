//! BitDinghy's shared engine.
//!
//! Both programs (`dinghy`, the command line, and `BitDinghy`, the window) are thin front-ends
//! over this crate. Anything that matters, such as the download list, the queue, settings and
//! the safety checks, belongs here so it works identically in both.
//!
//! This crate is also the ONLY place allowed to talk to the BitTorrent engine (`librqbit`).
//! If we ever need to upgrade or swap that engine, only this crate changes.

/// The program's display name.
pub const APP_NAME: &str = "BitDinghy";

/// This build's version. Cargo fills it in at compile time from `Cargo.toml`
/// (like `AssemblyVersion` in .NET, but with no extra step).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The one-line identity shown by `dinghy --version` and at the top of the window.
pub fn about() -> String {
    format!("{APP_NAME} {VERSION}")
}

/// Name of the BitTorrent engine type we wrap.
///
/// Skeleton-only: it proves the engine compiles and links on every platform we build on.
/// `std::any::type_name` just prints the type's full path, so it needs no engine setup.
pub fn engine_type() -> &'static str {
    std::any::type_name::<librqbit::Session>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_contains_name_and_version() {
        let text = about();
        assert!(text.contains(APP_NAME));
        assert!(text.contains(VERSION));
    }

    #[test]
    fn engine_is_librqbit() {
        assert!(engine_type().contains("librqbit"));
    }
}
