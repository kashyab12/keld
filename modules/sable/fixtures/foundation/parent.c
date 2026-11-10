#include <unistd.h>
extern int leaf_ready(void);
static int ready;
__attribute__((constructor)) static void parent_init(void) {
    ready = leaf_ready();
    write(1, "INIT parent\n", 12);
}
int parent_ready(void) { return ready; }
