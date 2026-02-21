// Test __builtin_types_compatible_p and __builtin_choose_expr
// EXPECT: 42

int main(void) {
    // Same types → 1
    int same = __builtin_types_compatible_p(int, int);
    
    // Different types → 0
    int diff = __builtin_types_compatible_p(int, long);
    
    // __builtin_choose_expr: select based on constant condition
    int chosen = __builtin_choose_expr(1, 10, 20);  // Should be 10
    
    // chosen=10, same=1, diff=0
    // 10 + 1 + 0 = 11
    // We need 42, so add 31
    return chosen + same + diff + 31;
}
