// EXPECT: 42
// Test casting between different pointer types

struct Data {
    int x;
    int y;
};

int main() {
    struct Data d;
    d.x = 10;
    d.y = 32;
    
    // Cast struct pointer to int pointer
    int *p = (int*)&d;
    
    // Access first field through cast pointer
    int first = *p;      // 10
    
    // Access second field
    int second = *(p + 1);  // 32
    
    return first + second;  // 42
}
