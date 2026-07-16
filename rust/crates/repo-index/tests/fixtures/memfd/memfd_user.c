/**
 * memfd_create usage patterns.
 * BI-LX-4 test fixture.
 */

#define _GNU_SOURCE
#include <sys/mman.h>
#include <unistd.h>
#include <stdio.h>
#include <string.h>
#include <fcntl.h>

/* Create basic memfd */
int create_basic_memfd(void) {
    int fd = memfd_create("basic_buffer", 0);
    if (fd < 0) {
        perror("memfd_create");
        return -1;
    }

    /* Size it */
    if (ftruncate(fd, 4096) < 0) {
        perror("ftruncate");
        close(fd);
        return -1;
    }

    return fd;
}

/* Create memfd with close-on-exec */
int create_cloexec_memfd(void) {
    int fd = memfd_create("cloexec_buffer", MFD_CLOEXEC);
    if (fd < 0) {
        return -1;
    }
    return fd;
}

/* Create sealable memfd */
int create_sealable_memfd(void) {
    int fd = memfd_create("sealable_buffer", MFD_ALLOW_SEALING);
    if (fd < 0) {
        return -1;
    }

    /* Could seal with fcntl(fd, F_ADD_SEALS, F_SEAL_WRITE) */
    return fd;
}

/* Create memfd with combined flags */
int create_full_memfd(void) {
    int fd = memfd_create("full_buffer", MFD_CLOEXEC | MFD_ALLOW_SEALING);
    if (fd < 0) {
        return -1;
    }
    return fd;
}

/* Map a memfd for use */
void *map_memfd(int fd, size_t size) {
    void *ptr = mmap(NULL, size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (ptr == MAP_FAILED) {
        return NULL;
    }
    return ptr;
}

int main(void) {
    int fd = create_basic_memfd();
    if (fd >= 0) {
        void *ptr = map_memfd(fd, 4096);
        if (ptr) {
            strcpy(ptr, "Hello from memfd");
            munmap(ptr, 4096);
        }
        close(fd);
    }
    return 0;
}
