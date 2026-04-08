#include <stdio.h>

static void do_handle(void) {
    printf("handling\n");
}

__attribute__((constructor))
static void register_my_handler(void) {
    /* registration */
}

int unused_func(void) {
    return 42;
}
