// EXPECT: 20
// Test goto with forward and backward jumps

int main() {
    int x = 0;
    
    goto middle;
    
start:
    x = x + 5;
    if (x < 15) {
        goto middle;
    }
    goto end;
    
middle:
    x = x + 5;
    goto start;
    
end:
    return x;
}
