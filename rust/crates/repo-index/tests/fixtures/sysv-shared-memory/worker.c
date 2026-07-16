/**
 * SysV shared memory worker/consumer.
 * BI-LX-1 test fixture.
 */

#include <sys/ipc.h>
#include <sys/shm.h>
#include <stdio.h>

#define SHM_KEY 0x1234

int read_shared_memory(void) {
    /* Open existing shared memory segment */
    int shmid = shmget(SHM_KEY, 0, 0);
    if (shmid < 0) {
        perror("shmget");
        return -1;
    }

    /* Attach read-only */
    char *data = shmat(shmid, NULL, SHM_RDONLY);
    if (data == (char *)-1) {
        perror("shmat");
        return -1;
    }

    /* Read data */
    printf("Read: %s\n", data);

    /* Detach */
    shmdt(data);

    return 0;
}

int main(void) {
    return read_shared_memory();
}
