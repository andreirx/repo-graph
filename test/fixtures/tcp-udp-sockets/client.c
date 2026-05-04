// BI-1B fixture: TCP client
// Expected: 1 tcp_socket consumer surface (socket + connect)

#include <sys/socket.h>
#include <netinet/in.h>

void connect_to_server(void) {
    int fd = socket(AF_INET, SOCK_STREAM, 0);

    struct sockaddr_in addr;
    addr.sin_family = AF_INET;
    addr.sin_port = 8080;

    connect(fd, (struct sockaddr*)&addr, sizeof(addr));

    send(fd, "hello", 5, 0);
    char buf[256];
    recv(fd, buf, sizeof(buf), 0);
}
