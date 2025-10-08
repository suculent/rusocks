//! Version information for the rusocks library

/// Current version of the rusocks library
pub const VERSION: &str = "v1.7.2";

/// Platform information (OS/architecture)
pub const PLATFORM: &str = {
    #[cfg(target_os = "windows")]
    {
        concat!("windows/", env!("CARGO_CFG_TARGET_ARCH"))
    }
    #[cfg(target_os = "macos")]
    {
        concat!("darwin/", env!("CARGO_CFG_TARGET_ARCH"))
    }
    #[cfg(target_os = "linux")]
    {
        concat!("linux/", env!("CARGO_CFG_TARGET_ARCH"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        concat!(
            env!("CARGO_CFG_TARGET_OS"),
            "/",
            env!("CARGO_CFG_TARGET_ARCH")
        )
    }
};
