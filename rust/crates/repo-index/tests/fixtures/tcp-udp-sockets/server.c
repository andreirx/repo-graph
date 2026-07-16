// BI-1B fixture: TCP server
// Expected: 1 tcp_socket provider surface (socket + bind + listen)

#include <sys/socket.h>
#include <netinet/in.h>

void start_tcp_server(void) {
    int fd = socket(AF_INET, SOCK_STREAM, 0);

    struct sockaddr_in addr;
    addr.sin_family = AF_INET;
    addr.sin_port = 8080;
    addr.sin_addr.s_addr = 0;

    bind(fd, (struct sockaddr*)&addr, sizeof(addr));
    listen(fd, 5);

    int client = accept(fd, NULL, NULL);
    // handle client
}
