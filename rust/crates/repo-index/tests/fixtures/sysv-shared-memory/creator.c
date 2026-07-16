/**
 * SysV shared memory creator/producer.
 * BI-LX-1 test fixture.
 */

#include <sys/ipc.h>
#include <sys/shm.h>
#include <stdio.h>
#include <string.h>

#define SHM_KEY 0x1234
#define SHM_SIZE 4096

int create_and_write(void) {
    /* Create shared memory segment */
    int shmid = shmget(SHM_KEY, SHM_SIZE, IPC_CREAT | 0644);
    if (shmid < 0) {
        perror("shmget");
        return -1;
    }

    /* Attach to segment */
    char *data = shmat(shmid, NULL, 0);
    if (data == (char *)-1) {
        perror("shmat");
        return -1;
    }

    /* Write data */
    strcpy(data, "Hello from creator");

    /* Detach */
    shmdt(data);

    return 0;
}

int main(void) {
    return create_and_write();
}
