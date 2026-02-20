// Test interrupt handler with more complex signal usage
// TODO: This test uses struct sigaction which is not yet supported
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

volatile sig_atomic_t interrupt_count = 0;

void interrupt_handler(int sig) {
    interrupt_count++;
    printf("Interrupt received (count: %d)\n", interrupt_count);
    if (interrupt_count >= 3) {
        printf("Too many interrupts, exiting\n");
        exit(0);
    }
}

int main() {
    struct sigaction sa;
    sa.sa_handler = interrupt_handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = 0;
    
    if (sigaction(SIGINT, &sa, NULL) == -1) {
        perror("sigaction");
        return 1;
    }
    
    printf("Signal handler registered. Press Ctrl+C to test.\n");
    printf("Will exit after 3 interrupts.\n");
    
    while (1) {
        sleep(1);
    }
    
    return 0;
}
