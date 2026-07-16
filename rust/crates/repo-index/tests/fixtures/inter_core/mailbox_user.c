// BI-EM-1 fixture: Linux kernel mailbox framework usage
// Simulates a driver that uses hardware mailbox for inter-core communication.

#include <linux/mailbox_client.h>
#include <linux/module.h>

struct mbox_client client;
struct mbox_chan *chan;

int mailbox_init(void)
{
    // Request a mailbox channel - bidirectional setup
    chan = mbox_request_channel(&client, 0);
    if (IS_ERR(chan))
        return PTR_ERR(chan);
    return 0;
}

int mailbox_init_byname(void)
{
    // Request channel by name - arg1 is the name
    chan = mbox_request_channel_byname(&client, "mcu-mbox");
    if (IS_ERR(chan))
        return PTR_ERR(chan);
    return 0;
}

int mailbox_send(void *data)
{
    // Send message to remote core - provider role
    return mbox_send_message(chan, data);
}

void mailbox_tx_done(void)
{
    // TX completion notification - provider role
    mbox_client_txdone(chan, 0);
}

bool mailbox_peek(void)
{
    // Check for pending data - consumer role
    return mbox_client_peek_data(chan);
}

void mailbox_exit(void)
{
    // Release the channel
    mbox_free_channel(chan);
}
