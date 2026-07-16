/**
 * SysV message queue cleanup utility.
 * BI-LX-2 test fixture.
 */

#include <sys/ipc.h>
#include <sys/msg.h>
#include <stdio.h>

#define MSG_KEY 0x5678

int cleanup_message_queue(void) {
    /* Open existing message queue */
    int msqid = msgget(MSG_KEY, 0);
    if (msqid < 0) {
        /* Queue doesn't exist, nothing to clean */
        return 0;
    }

    /* Remove the queue */
    if (msgctl(msqid, IPC_RMID, NULL) < 0) {
        perror("msgctl IPC_RMID");
        return -1;
    }

    printf("Message queue removed\n");
    return 0;
}

int main(void) {
    return cleanup_message_queue();
}
