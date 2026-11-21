pub fn supports_platform(os: &str, arch: &str) -> bool {
    os == "macos" && arch == "aarch64"
}

#[cfg(any(test, not(all(target_os = "macos", target_arch = "aarch64"))))]
pub(crate) fn unsupported_message() -> &'static str {
    "MXC Seatbelt worker requires macOS aarch64; this host is unsupported"
}

#[cfg(test)]
mod tests {
    use super::supports_platform;
    use super::unsupported_message;

    #[test]
    fn only_macos_aarch64_is_supported() {
        assert!(supports_platform("macos", "aarch64"));
        assert!(!supports_platform("macos", "x86_64"));
        assert!(!supports_platform("linux", "aarch64"));
        assert!(!supports_platform("windows", "x86_64"));
        assert!(!supports_platform("Darwin", "arm64"));
        assert!(!supports_platform("", ""));
    }

    #[test]
    fn unsupported_message_names_the_required_host() {
        let message = unsupported_message();
        assert!(message.contains("macOS aarch64"));
        assert!(message.contains("unsupported"));
    }
}
