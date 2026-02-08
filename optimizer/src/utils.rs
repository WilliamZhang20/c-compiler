/// Utility functions for optimization passes

/// Check if a number is a power of 2
#[inline]
pub fn is_power_of_two(n: i64) -> bool {
    n > 0 && (n & (n - 1)) == 0
}

/// Calculate log2 of a power of 2
#[inline]
pub fn log2(n: i64) -> i64 {
    let mut count = 0;
    let mut num = n;
    while num > 1 {
        num >>= 1;
        count += 1;
    }
    count
}
