// C-SB-1 test: fopen with read mode
#include <stdio.h>

void read_config(void) {
    FILE* f = fopen("/etc/config.txt", "r");
    if (f) {
        fclose(f);
    }
}

void read_binary(void) {
    FILE* f = fopen("/data/binary.dat", "rb");
    if (f) {
        fclose(f);
    }
}
