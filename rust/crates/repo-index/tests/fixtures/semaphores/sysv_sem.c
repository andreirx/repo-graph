/* SysV semaphore fixture for BI-LX-3 */
#include <sys/types.h>
#include <sys/ipc.h>
#include <sys/sem.h>

#define SEM_KEY 0x1234
#define NSEMS 2

/* Create or open a semaphore set */
int create_semaphore_set(void) {
    int semid = semget(SEM_KEY, NSEMS, IPC_CREAT | 0644);
    return semid;
}

/* Acquire semaphore (P operation / wait / decrement) */
void sem_acquire(int semid, int sem_num) {
    struct sembuf sop;
    sop.sem_num = sem_num;
    sop.sem_op = -1;  /* decrement = acquire */
    sop.sem_flg = 0;
    semop(semid, &sop, 1);
}

/* Release semaphore (V operation / signal / increment) */
void sem_release(int semid, int sem_num) {
    struct sembuf sop;
    sop.sem_num = sem_num;
    sop.sem_op = 1;   /* increment = release */
    sop.sem_flg = 0;
    semop(semid, &sop, 1);
}

/* Timed acquire with timeout */
void sem_acquire_timed(int semid, int sem_num) {
    struct sembuf sop;
    struct timespec ts = { .tv_sec = 5, .tv_nsec = 0 };
    sop.sem_num = sem_num;
    sop.sem_op = -1;
    sop.sem_flg = 0;
    semtimedop(semid, &sop, 1, &ts);
}

/* Get semaphore value */
int get_sem_value(int semid, int sem_num) {
    return semctl(semid, sem_num, GETVAL);
}

/* Remove semaphore set */
void remove_semaphore_set(int semid) {
    semctl(semid, 0, IPC_RMID);
}

/* Example usage */
int main(void) {
    int semid = create_semaphore_set();
    if (semid < 0) return 1;

    /* Critical section with semaphore */
    sem_acquire(semid, 0);
    /* ... critical section ... */
    sem_release(semid, 0);

    /* Cleanup */
    remove_semaphore_set(semid);
    return 0;
}
