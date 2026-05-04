// BI-1D fixture: Signal handlers
// Expected: 3 process_signal consumer surfaces (signal, sigaction, sigwait)

#include <signal.h>
#include <pthread.h>

volatile sig_atomic_t shutdown_requested = 0;

void sigterm_handler(int sig) {
    shutdown_requested = 1;
}

void sigint_handler(int sig) {
    // Handle Ctrl+C
}

void setup_handlers(void) {
    // Legacy signal registration
    signal(SIGTERM, sigterm_handler);

    // Modern sigaction registration
    struct sigaction act;
    act.sa_handler = sigint_handler;
    sigemptyset(&act.sa_mask);
    act.sa_flags = 0;
    sigaction(SIGINT, &act, NULL);
}

void wait_for_shutdown(void) {
    sigset_t set;
    int sig;
    sigemptyset(&set);
    sigaddset(&set, SIGTERM);
    sigaddset(&set, SIGINT);
    sigwait(&set, &sig);
}
