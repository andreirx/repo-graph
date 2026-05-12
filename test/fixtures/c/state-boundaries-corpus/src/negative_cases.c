// C-SB-1 test: negative cases (should NOT produce ResolvedCallsite)
#include <stdio.h>

// printf is not state-boundary-relevant
void log_message(void) {
    printf("Hello, world!\n");
}

// Dynamic paths should not produce callsite
void dynamic_path(char* path) {
    FILE* f = fopen(path, "r");
    if (f) {
        fclose(f);
    }
}

// malloc is not state-boundary-relevant
void allocate(void) {
    void* ptr = malloc(1024);
    if (ptr) {
        free(ptr);
    }
}
