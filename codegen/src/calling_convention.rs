// Calling convention abstraction for cross-platform support
use crate::x86::X86Reg;
use model::{CallingConvention as ConventionType, Platform};

/// Trait defining platform-specific calling conventions
pub trait CallingConvention {
    /// Registers used for integer/pointer parameters (in order)
    fn param_regs(&self) -> &'static [X86Reg];
    
    /// Registers used for floating-point parameters (in order)
    fn float_param_regs(&self) -> &'static [X86Reg];
    
    /// Register used for return values
    #[allow(dead_code)]
    fn return_reg(&self) -> X86Reg {
        X86Reg::Rax
    }
    
    /// Register used for floating-point return values
    #[allow(dead_code)]
    fn float_return_reg(&self) -> X86Reg {
        X86Reg::Xmm0
    }
    
    /// Size of shadow/home space for register parameters (in bytes)
    fn shadow_space_size(&self) -> usize;
    
    /// Callee-saved registers (must be preserved across function calls)
    #[allow(dead_code)]
    fn callee_saved_regs(&self) -> &'static [X86Reg];
}

/// Windows x64 (Microsoft) calling convention
pub struct WindowsX64Convention;

impl CallingConvention for WindowsX64Convention {
    fn param_regs(&self) -> &'static [X86Reg] {
        &[X86Reg::Rcx, X86Reg::Rdx, X86Reg::R8, X86Reg::R9]
    }
    
    fn float_param_regs(&self) -> &'static [X86Reg] {
        &[X86Reg::Xmm0, X86Reg::Xmm1, X86Reg::Xmm2, X86Reg::Xmm3]
    }
    
    fn shadow_space_size(&self) -> usize {
        32  // 4 registers × 8 bytes
    }
    
    fn callee_saved_regs(&self) -> &'static [X86Reg] {
        &[X86Reg::Rbx, X86Reg::Rsi, X86Reg::Rdi, 
          X86Reg::R12, X86Reg::R13, X86Reg::R14, X86Reg::R15]
    }
}

/// System V AMD64 ABI (Linux, BSD, macOS)
pub struct SystemVConvention;

impl CallingConvention for SystemVConvention {
    fn param_regs(&self) -> &'static [X86Reg] {
        &[X86Reg::Rdi, X86Reg::Rsi, X86Reg::Rdx, 
          X86Reg::Rcx, X86Reg::R8, X86Reg::R9]
    }
    
    fn float_param_regs(&self) -> &'static [X86Reg] {
        &[X86Reg::Xmm0, X86Reg::Xmm1, X86Reg::Xmm2, X86Reg::Xmm3,
          X86Reg::Xmm4, X86Reg::Xmm5, X86Reg::Xmm6, X86Reg::Xmm7]
    }
    
    fn shadow_space_size(&self) -> usize {
        0  // No shadow space in System V
    }
    
    fn callee_saved_regs(&self) -> &'static [X86Reg] {
        &[X86Reg::Rbx, X86Reg::R12, X86Reg::R13, X86Reg::R14, X86Reg::R15]
    }
}

/// Get the appropriate calling convention for a platform
pub fn get_convention(convention_type: ConventionType) -> Box<dyn CallingConvention> {
    match convention_type {
        ConventionType::WindowsX64 => Box::new(WindowsX64Convention),
        ConventionType::SystemV => Box::new(SystemVConvention),
    }
}

/// Helper to get calling convention for current host
#[allow(dead_code)]
pub fn host_convention() -> Box<dyn CallingConvention> {
    let platform = Platform::host();
    let convention_type = ConventionType::for_platform(platform);
    get_convention(convention_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_convention() {
        let conv = WindowsX64Convention;
        assert_eq!(conv.param_regs().len(), 4);
        assert_eq!(conv.param_regs()[0], X86Reg::Rcx);
        assert_eq!(conv.shadow_space_size(), 32);
        assert!(conv.callee_saved_regs().contains(&X86Reg::Rbx));
        assert!(conv.callee_saved_regs().contains(&X86Reg::Rsi));
    }

    #[test]
    fn test_systemv_convention() {
        let conv = SystemVConvention;
        assert_eq!(conv.param_regs().len(), 6);
        assert_eq!(conv.param_regs()[0], X86Reg::Rdi);
        assert_eq!(conv.shadow_space_size(), 0);
        assert!(conv.callee_saved_regs().contains(&X86Reg::Rbx));
        assert!(!conv.callee_saved_regs().contains(&X86Reg::Rsi)); // Not callee-saved in System V
    }

    #[test]
    fn test_get_convention() {
        let windows = get_convention(ConventionType::WindowsX64);
        assert_eq!(windows.shadow_space_size(), 32);
        
        let systemv = get_convention(ConventionType::SystemV);
        assert_eq!(systemv.shadow_space_size(), 0);
    }

    #[test]
    fn test_host_convention() {
        let conv = host_convention();
        
        #[cfg(target_os = "windows")]
        assert_eq!(conv.shadow_space_size(), 32);
        
        #[cfg(target_os = "linux")]
        assert_eq!(conv.shadow_space_size(), 0);
    }
}
