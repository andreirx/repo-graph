// C-SB-1 test: fopen with read-write modes
#include <stdio.h>

void update_data(void) {
    FILE* f = fopen("/data/state.dat", "r+");
    if (f) {
        // Read and update
        fclose(f);
    }
}

void rewrite_data(void) {
    FILE* f = fopen("/data/state.dat", "w+");
    if (f) {
        // Truncate and read/write
        fclose(f);
    }
}
