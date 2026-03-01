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

/// SIMD instruction set level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SimdLevel {
    /// No SIMD (scalar only)
    None,
    /// SSE2: 128-bit registers, baseline for x86-64
    SSE2,
    /// SSE4.1: adds integer multiply, blend, etc.
    SSE41,
    /// AVX: 256-bit registers, 3-operand encoding
    AVX,
    /// AVX2: 256-bit integer operations
    AVX2,
}

impl SimdLevel {
    /// Detect the SIMD level supported by the current CPU
    pub fn detect() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            if is_x86_feature_detected!("avx2") {
                return SimdLevel::AVX2;
            }
            if is_x86_feature_detected!("avx") {
                return SimdLevel::AVX;
            }
            if is_x86_feature_detected!("sse4.1") {
                return SimdLevel::SSE41;
            }
            // SSE2 is always available on x86-64
            return SimdLevel::SSE2;
        }
        #[cfg(not(target_arch = "x86_64"))]
        SimdLevel::None
    }

    /// Number of 32-bit elements per vector register
    pub fn vector_width_32(self) -> usize {
        match self {
            SimdLevel::None => 1,
            SimdLevel::SSE2 | SimdLevel::SSE41 => 4,
            SimdLevel::AVX | SimdLevel::AVX2 => 8,
        }
    }

    /// Whether this level supports 256-bit operations
    pub fn has_256bit(self) -> bool {
        self >= SimdLevel::AVX
    }
}

/// Complete target configuration
#[derive(Debug, Clone)]
pub struct TargetConfig {
    pub platform: Platform,
    pub calling_convention: CallingConvention,
    pub simd_level: SimdLevel,
    /// When true, do not use the 128-byte red zone below RSP (kernel code).
    pub no_red_zone: bool,
    /// When true, do not emit SSE/FPU instructions (kernel code).
    pub no_sse: bool,
}

impl TargetConfig {
    /// Create configuration for the host platform
    pub fn host() -> Self {
        let platform = Platform::host();
        Self {
            platform,
            calling_convention: CallingConvention::for_platform(platform),
            simd_level: SimdLevel::detect(),
            no_red_zone: false,
            no_sse: false,
        }
    }

    /// Create configuration for a specific platform
    pub fn for_platform(platform: Platform) -> Self {
        Self {
            platform,
            calling_convention: CallingConvention::for_platform(platform),
            simd_level: SimdLevel::detect(),
            no_red_zone: false,
            no_sse: false,
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
