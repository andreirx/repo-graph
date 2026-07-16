/* Named POSIX semaphore fixture for BI-LX-3 */
#include <fcntl.h>
#include <semaphore.h>
#include <sys/stat.h>

#define SEM_NAME "/test_semaphore"

/* Create or open a named semaphore */
sem_t *create_named_semaphore(void) {
    sem_t *sem = sem_open(SEM_NAME, O_CREAT, 0644, 1);
    return sem;
}

/* Open existing named semaphore */
sem_t *open_named_semaphore(void) {
    sem_t *sem = sem_open(SEM_NAME, 0);
    return sem;
}

/* Close a named semaphore (does not remove it) */
void close_named_semaphore(sem_t *sem) {
    sem_close(sem);
}

/* Remove a named semaphore from the system */
void unlink_named_semaphore(void) {
    sem_unlink(SEM_NAME);
}

/* Example producer process */
int producer_main(void) {
    sem_t *sem = create_named_semaphore();
    if (sem == (sem_t *)-1) return 1;

    /* Semaphore is now available for other processes */
    /* NOTE: sem_wait/sem_post are deferred in BI-LX-3 */
    /* They would require identity correlation to be IPC-safe */

    close_named_semaphore(sem);
    return 0;
}

/* Example consumer process */
int consumer_main(void) {
    sem_t *sem = open_named_semaphore();
    if (sem == (sem_t *)-1) return 1;

    /* Use the shared semaphore */
    /* NOTE: sem_wait/sem_post are deferred in BI-LX-3 */

    close_named_semaphore(sem);
    return 0;
}

/* Cleanup process */
int cleanup_main(void) {
    unlink_named_semaphore();
    return 0;
}

int main(void) {
    /* Create, use, and cleanup */
    producer_main();
    consumer_main();
    cleanup_main();
    return 0;
}
