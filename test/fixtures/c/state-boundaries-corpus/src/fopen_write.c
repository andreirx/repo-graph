// C-SB-1 test: fopen with write modes
#include <stdio.h>

void write_log(void) {
    FILE* f = fopen("/var/log/app.log", "w");
    if (f) {
        fprintf(f, "Log entry\n");
        fclose(f);
    }
}

void append_log(void) {
    FILE* f = fopen("/var/log/app.log", "a");
    if (f) {
        fprintf(f, "Appended entry\n");
        fclose(f);
    }
}
