// BI-EM-1 fixture: Linux kernel RPMsg framework usage
// Simulates a driver that uses RPMsg for inter-core communication.

#include <linux/rpmsg.h>
#include <linux/module.h>

struct rpmsg_device *rpdev;
struct rpmsg_endpoint *ept;

static int rpmsg_callback(struct rpmsg_device *rpdev, void *data,
                          int len, void *priv, u32 src)
{
    // Callback for received messages
    return 0;
}

int rpmsg_init(struct rpmsg_device *dev)
{
    rpdev = dev;

    // Create an endpoint - bidirectional setup
    ept = rpmsg_create_ept(rpdev, rpmsg_callback, NULL, RPMSG_ADDR_ANY);
    if (!ept)
        return -ENOMEM;
    return 0;
}

int rpmsg_send_data(void *data, int len)
{
    // Send message - provider role
    return rpmsg_send(ept, data, len);
}

int rpmsg_send_to_addr(void *data, int len, u32 dst)
{
    // Send to specific address - provider role
    return rpmsg_sendto(ept, data, len, dst);
}

int rpmsg_send_offchan(void *data, int len, u32 src, u32 dst)
{
    // Send via specific src/dst - provider role
    return rpmsg_send_offchannel(ept, src, dst, data, len);
}

int rpmsg_try_send(void *data, int len)
{
    // Non-blocking send - provider role
    return rpmsg_trysend(ept, data, len);
}

int rpmsg_try_send_to(void *data, int len, u32 dst)
{
    // Non-blocking send to address - provider role
    return rpmsg_trysendto(ept, data, len, dst);
}

// Note: RPMsg receive is callback-based via rpmsg_callback registered
// in rpmsg_create_ept(). There is no rpmsg_recv() API in Linux kernel.

void rpmsg_exit(void)
{
    // Destroy endpoint
    rpmsg_destroy_ept(ept);
}

static int rpmsg_probe(struct rpmsg_device *rpdev)
{
    // Register device with RPMsg subsystem
    return rpmsg_register_device(rpdev);
}
