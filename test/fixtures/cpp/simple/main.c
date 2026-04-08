#include <stdio.h>
#include "util.h"

static int internal_func(void) {
    return 42;
}

int helper(int a, int b) {
    return a + b;
}

int main(int argc, char *argv[]) {
    Point p = {1, 2};
    printf("sum: %d\n", helper(p.x, p.y));
    printf("internal: %d\n", internal_func());
    return 0;
}
