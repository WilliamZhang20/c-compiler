// Test various escape sequences
// EXPECT: 0
#include <stdio.h>

int main() {
    printf("Line 1\nLine 2\n");
    printf("Tab:\there\n");
    printf("Quote: \"Hello\"\n");
    printf("Backslash: \\\n");
    return 0;
}
