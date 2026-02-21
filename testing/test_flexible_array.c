// Test flexible array members
// EXPECT: 42

struct FlexArray {
    int len;
    int data[];
};

int main(void) {
    // sizeof should not include the flexible array member
    int sz = sizeof(struct FlexArray);
    
    // sz should be 4 (just the int len field, possibly with padding)
    // On most systems sizeof is 4 for this struct
    // We'll just use a simple calculation
    return sz + 38;  // 4 + 38 = 42
}
