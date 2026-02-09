// EXPECT: 10
// Test goto with loop (Duff's device style)

int main() {
    int count = 10;
    int iterations = 0;
    
loop_start:
    if (count > 0) {
        iterations = iterations + 1;
        count = count - 1;
        goto loop_start;
    }
    
    return iterations;
}
