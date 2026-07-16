/**
 * SysV shared memory cleanup utility.
 * BI-LX-1 test fixture.
 */

#include <sys/ipc.h>
#include <sys/shm.h>
#include <stdio.h>

#define SHM_KEY 0x1234

int cleanup_shared_memory(void) {
    /* Open existing shared memory segment */
    int shmid = shmget(SHM_KEY, 0, 0);
    if (shmid < 0) {
        /* Segment doesn't exist, nothing to clean */
        return 0;
    }

    /* Remove the segment */
    if (shmctl(shmid, IPC_RMID, NULL) < 0) {
        perror("shmctl IPC_RMID");
        return -1;
    }

    printf("Shared memory segment removed\n");
    return 0;
}

int main(void) {
    return cleanup_shared_memory();
}
