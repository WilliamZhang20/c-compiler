// EXPECT: 42
// Test union types with overlapping memory
union Data {
    int i;
    char c;
};

int main() {
    union Data d;
    
    // Set as int
    d.i = 0x12345678;
    
    // Read as char (should get low byte on little-endian: 0x78 = 120)
    char low_byte = d.c;
    
    // Set as char
    d.c = 42;
    
    // Now d.i's low byte is 42, but upper bytes are undefined
    // Return the char value we set
    return d.c;
}
