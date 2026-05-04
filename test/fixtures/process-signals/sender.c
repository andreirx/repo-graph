// BI-1D fixture: Signal sender
// Expected: 2 process_signal provider surfaces (kill + raise)

#include <signal.h>
#include <unistd.h>

void terminate_child(pid_t child) {
    kill(child, SIGTERM);
}

void request_shutdown(void) {
    raise(SIGUSR1);
}
