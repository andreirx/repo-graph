// BI-1B fixture: UDP sender/receiver
// Expected: 2 udp_socket surfaces (send + receive)

#include <sys/socket.h>
#include <netinet/in.h>

void send_broadcast(void) {
    int fd = socket(AF_INET, SOCK_DGRAM, 0);

    struct sockaddr_in dest;
    dest.sin_family = AF_INET;
    dest.sin_port = 5353;

    sendto(fd, "discovery", 9, 0, (struct sockaddr*)&dest, sizeof(dest));
}

void receive_responses(void) {
    int fd = socket(AF_INET6, SOCK_DGRAM, 0);

    struct sockaddr_in6 addr;
    addr.sin6_family = AF_INET6;
    addr.sin6_port = 5353;

    bind(fd, (struct sockaddr*)&addr, sizeof(addr));

    char buf[1024];
    struct sockaddr_in6 sender;
    socklen_t len = sizeof(sender);
    recvfrom(fd, buf, sizeof(buf), 0, (struct sockaddr*)&sender, &len);
}
