/**
 * SysV message queue sender.
 * BI-LX-2 test fixture.
 */

#include <sys/ipc.h>
#include <sys/msg.h>
#include <string.h>
#include <stdio.h>

#define MSG_KEY 0x5678

struct message {
    long mtype;
    char mtext[256];
};

int send_message(void) {
    /* Create or open message queue */
    int msqid = msgget(MSG_KEY, IPC_CREAT | 0644);
    if (msqid < 0) {
        perror("msgget");
        return -1;
    }

    /* Prepare and send message */
    struct message msg;
    msg.mtype = 1;
    strcpy(msg.mtext, "Hello from sender");

    if (msgsnd(msqid, &msg, sizeof(msg.mtext), 0) < 0) {
        perror("msgsnd");
        return -1;
    }

    printf("Message sent\n");
    return 0;
}

int main(void) {
    return send_message();
}
