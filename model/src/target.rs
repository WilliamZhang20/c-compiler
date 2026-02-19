// Target platform configuration for cross-platform support

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Linux,
}

impl Platform {
    /// Detect the current compilation target platform
    pub fn host() -> Self {
        if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "linux") {
            Platform::Linux
        } else {
            // Default to Linux for other Unix-like systems
            Platform::Linux
        }
    }

    /// Get the executable file extension for this platform
    pub fn executable_extension(&self) -> &'static str {
        match self {
            Platform::Windows => ".exe",
            Platform::Linux => "",
        }
    }

    /// Check if this platform requires console subsystem flag
    pub fn needs_console_flag(&self) -> bool {
        matches!(self, Platform::Windows)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallingConvention {
    WindowsX64,  // Microsoft x64 calling convention
    SystemV,     // System V AMD64 ABI (Linux, BSD, macOS)
}

impl CallingConvention {
    /// Get the calling convention for a platform
    pub fn for_platform(platform: Platform) -> Self {
        match platform {
            Platform::Windows => CallingConvention::WindowsX64,
            Platform::Linux => CallingConvention::SystemV,
        }
    }

    /// Size of shadow space (home space) for register parameters
    pub fn shadow_space_size(&self) -> usize {
        match self {
            CallingConvention::WindowsX64 => 32, // 4 registers × 8 bytes
            CallingConvention::SystemV => 0,      // No shadow space
        }
    }
}

/// Complete target configuration
#[derive(Debug, Clone)]
pub struct TargetConfig {
    pub platform: Platform,
    pub calling_convention: CallingConvention,
}

impl TargetConfig {
    /// Create configuration for the host platform
    pub fn host() -> Self {
        let platform = Platform::host();
        Self {
            platform,
            calling_convention: CallingConvention::for_platform(platform),
        }
    }

    /// Create configuration for a specific platform
    pub fn for_platform(platform: Platform) -> Self {
        Self {
            platform,
            calling_convention: CallingConvention::for_platform(platform),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_detection() {
        let platform = Platform::host();
        #[cfg(target_os = "windows")]
        assert_eq!(platform, Platform::Windows);
        #[cfg(target_os = "linux")]
        assert_eq!(platform, Platform::Linux);
    }

    #[test]
    fn test_executable_extension() {
        assert_eq!(Platform::Windows.executable_extension(), ".exe");
        assert_eq!(Platform::Linux.executable_extension(), "");
    }

    #[test]
    fn test_calling_convention() {
        let windows_cc = CallingConvention::for_platform(Platform::Windows);
        assert_eq!(windows_cc, CallingConvention::WindowsX64);
        assert_eq!(windows_cc.shadow_space_size(), 32);

        let linux_cc = CallingConvention::for_platform(Platform::Linux);
        assert_eq!(linux_cc, CallingConvention::SystemV);
        assert_eq!(linux_cc.shadow_space_size(), 0);
    }

    #[test]
    fn test_target_config() {
        let config = TargetConfig::host();
        assert_eq!(
            config.calling_convention,
            CallingConvention::for_platform(config.platform)
        );
    }
}
