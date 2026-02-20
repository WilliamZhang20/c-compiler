// Test signal handler
// EXPECT: 0
#include <signal.h>
#include <stdio.h>

void signal_handler(int signum) {
    printf("Caught signal %d\n", signum);
}

int main() {
    signal(SIGINT, signal_handler);
    printf("Signal handler registered\n");
    return 0;
}
