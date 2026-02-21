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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_of_two_basic() {
        assert!(is_power_of_two(1));
        assert!(is_power_of_two(2));
        assert!(is_power_of_two(4));
        assert!(is_power_of_two(8));
        assert!(is_power_of_two(16));
        assert!(is_power_of_two(256));
        assert!(is_power_of_two(1024));
    }

    #[test]
    fn not_power_of_two() {
        assert!(!is_power_of_two(0));
        assert!(!is_power_of_two(3));
        assert!(!is_power_of_two(5));
        assert!(!is_power_of_two(6));
        assert!(!is_power_of_two(7));
        assert!(!is_power_of_two(9));
        assert!(!is_power_of_two(15));
    }

    #[test]
    fn negative_not_power_of_two() {
        assert!(!is_power_of_two(-1));
        assert!(!is_power_of_two(-2));
        assert!(!is_power_of_two(-4));
    }

    #[test]
    fn large_power_of_two() {
        assert!(is_power_of_two(1 << 30)); // 2^30
        assert!(is_power_of_two(1 << 40)); // 2^40
    }

    #[test]
    fn log2_basic() {
        assert_eq!(log2(1), 0);
        assert_eq!(log2(2), 1);
        assert_eq!(log2(4), 2);
        assert_eq!(log2(8), 3);
        assert_eq!(log2(16), 4);
        assert_eq!(log2(256), 8);
        assert_eq!(log2(1024), 10);
    }

    #[test]
    fn log2_large() {
        assert_eq!(log2(1 << 20), 20);
        assert_eq!(log2(1 << 30), 30);
    }
}
