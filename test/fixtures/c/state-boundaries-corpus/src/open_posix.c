// C-SB-1 test: POSIX open() with flags
#include <fcntl.h>
#include <unistd.h>

void read_device(void) {
    int fd = open("/dev/input0", O_RDONLY);
    if (fd >= 0) {
        close(fd);
    }
}

void write_fifo(void) {
    int fd = open("/var/run/pipe", O_WRONLY);
    if (fd >= 0) {
        close(fd);
    }
}

void update_file(void) {
    int fd = open("/tmp/shared.dat", O_RDWR);
    if (fd >= 0) {
        close(fd);
    }
}
