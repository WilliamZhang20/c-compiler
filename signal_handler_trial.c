#include <stdio.h>
#include <signal.h>
#include <unistd.h>

volatile int alive = 1;

void handle_sigint(int sig) {
    printf("\nCaught signal %d (Ctrl+C)\n", sig);
    alive = 0;
}

int main() {
    signal(SIGINT, handle_sigint);

    while (alive) {
        printf("Running... press Ctrl+C\n");
        sleep(1);
    }

    printf("Time to die\n");

    return 0;
}