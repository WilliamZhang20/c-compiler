// Test .bss and .rodata section placement
// Uninitialized globals should go to .bss
// Const globals should go to .rodata
// Mutable initialized globals should go to .data

int uninit_global;              // .bss
int zero_global = 0;            // .bss (zero-initialized)
int init_global = 42;           // .data
const int const_global = 100;   // .rodata
static int static_uninit;       // .bss

int main() {
    // Test that all globals are accessible and have correct values
    uninit_global = 10;
    
    int result = 0;
    
    // uninit_global should have been set to 10
    if (uninit_global != 10) return 1;
    
    // zero_global should be 0 (BSS is zero-initialized)
    if (zero_global != 0) return 2;
    
    // init_global should be 42
    if (init_global != 42) return 3;
    
    // const_global should be 100
    if (const_global != 100) return 4;
    
    // static_uninit should be 0 (BSS is zero-initialized)
    if (static_uninit != 0) return 5;
    
    return 0;
}
