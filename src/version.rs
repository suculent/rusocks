//! Version information for the rusocks library

/// Current version of the rusocks library
pub const VERSION: &str = "v1.7.2";

/// Platform information (OS/architecture)
pub const PLATFORM: &str = {
    #[cfg(target_os = "windows")]
    {
        if cfg!(target_arch = "x86_64") {
            "windows/x86_64"
        } else if cfg!(target_arch = "aarch64") {
            "windows/aarch64"
        } else if cfg!(target_arch = "x86") {
            "windows/x86"
        } else {
            "windows/unknown"
        }
    }
    #[cfg(target_os = "macos")]
    {
        if cfg!(target_arch = "aarch64") {
            "darwin/aarch64"
        } else if cfg!(target_arch = "x86_64") {
            "darwin/x86_64"
        } else {
            "darwin/unknown"
        }
    }
    #[cfg(target_os = "linux")]
    {
        if cfg!(target_arch = "x86_64") {
            "linux/x86_64"
        } else if cfg!(target_arch = "aarch64") {
            "linux/aarch64"
        } else if cfg!(target_arch = "arm") {
            "linux/arm"
        } else if cfg!(target_arch = "riscv64") {
            "linux/riscv64"
        } else if cfg!(target_arch = "powerpc64") {
            "linux/powerpc64"
        } else if cfg!(target_arch = "s390x") {
            "linux/s390x"
        } else {
            "linux/unknown"
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        if cfg!(target_arch = "x86_64") {
            "unknown_os/x86_64"
        } else if cfg!(target_arch = "aarch64") {
            "unknown_os/aarch64"
        } else if cfg!(target_arch = "arm") {
            "unknown_os/arm"
        } else if cfg!(target_arch = "riscv64") {
            "unknown_os/riscv64"
        } else if cfg!(target_arch = "powerpc64") {
            "unknown_os/powerpc64"
        } else if cfg!(target_arch = "s390x") {
            "unknown_os/s390x"
        } else {
            "unknown_os/unknown_arch"
        }
    }
};
