#include <stdio.h>
#include <signal.h>
#include <windows.h>

volatile sig_atomic_t stop = 0;

void handle_sigint(int sig)
{
    stop = 1;   // set flag (safe inside signal handler)
}

int main(void)
{
    signal(SIGINT, handle_sigint);

    printf("Running... Press Ctrl+C to stop.\n");

    while (!stop)
    {
        printf("Working...\n");
        Sleep(1000);  // Windows sleep (milliseconds)
    }

    printf("SIGINT received. Cleaning up and exiting...\n");

    return 0;
}
