//! Utility functions for update client

use crate::error::UpdateError;
use crate::models::PackageInfo;

/// Select the best package for current platform from a list
pub fn select_package(packages: &[PackageInfo]) -> Result<PackageInfo, UpdateError> {
    let platform = crate::config::default_platform();
    packages
        .iter()
        .find(|p| matches_platform(p, &platform))
        .cloned()
        .ok_or(UpdateError::NoSuitablePackage)
}

/// Check if a package matches the given platform
pub fn matches_platform(package: &PackageInfo, platform: &str) -> bool {
    match package.platform {
        crate::models::Platform::Linux => platform.contains("linux"),
        crate::models::Platform::Windows => platform.contains("windows"),
        crate::models::Platform::MacOS => platform.contains("macos") || platform.contains("darwin"),
        crate::models::Platform::Wasm => platform.contains("wasm"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{PackageType, Platform};

    fn make_package(platform: Platform) -> PackageInfo {
        PackageInfo {
            id: "test-pkg".to_string(),
            platform,
            package_type: PackageType::Full,
            download_url: "/test".to_string(),
            hash: "abc".to_string(),
            size: 100,
            signature: "sig".to_string(),
            base_version: None,
        }
    }

    #[test]
    fn test_matches_platform_linux() {
        let pkg = make_package(Platform::Linux);
        assert!(matches_platform(&pkg, "linux"));
        assert!(matches_platform(&pkg, "linux_amd64"));
        assert!(!matches_platform(&pkg, "windows"));
        assert!(!matches_platform(&pkg, "macos"));
    }

    #[test]
    fn test_matches_platform_windows() {
        let pkg = make_package(Platform::Windows);
        assert!(matches_platform(&pkg, "windows"));
        assert!(!matches_platform(&pkg, "linux"));
    }

    #[test]
    fn test_matches_platform_macos() {
        let pkg = make_package(Platform::MacOS);
        assert!(matches_platform(&pkg, "macos"));
        assert!(matches_platform(&pkg, "darwin"));
        assert!(!matches_platform(&pkg, "linux"));
    }

    #[test]
    fn test_select_package_found() {
        let packages = vec![
            make_package(Platform::Linux),
            make_package(Platform::Windows),
        ];
        // default_platform() returns "linux" on linux
        // This test may behave differently on different platforms
        let result = select_package(&packages);
        assert!(result.is_ok());
    }

    #[test]
    fn test_select_package_not_found() {
        let packages = vec![make_package(Platform::Wasm)];
        // Should fail on non-wasm platforms
        if !cfg!(target_arch = "wasm32") {
            assert!(select_package(&packages).is_err());
        }
    }
}
