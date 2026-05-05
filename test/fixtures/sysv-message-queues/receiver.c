/**
 * SysV message queue receiver.
 * BI-LX-2 test fixture.
 */

#include <sys/ipc.h>
#include <sys/msg.h>
#include <stdio.h>

#define MSG_KEY 0x5678

struct message {
    long mtype;
    char mtext[256];
};

int receive_message(void) {
    /* Open existing message queue */
    int msqid = msgget(MSG_KEY, 0);
    if (msqid < 0) {
        perror("msgget");
        return -1;
    }

    /* Receive message (any type, no flags) */
    struct message msg;
    if (msgrcv(msqid, &msg, sizeof(msg.mtext), 0, 0) < 0) {
        perror("msgrcv");
        return -1;
    }

    printf("Received: %s\n", msg.mtext);
    return 0;
}

int main(void) {
    return receive_message();
}
