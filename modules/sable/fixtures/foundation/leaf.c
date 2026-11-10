#include <unistd.h>
static __thread unsigned initialized = 17;
static __thread unsigned zeroed;
static int ready;
__attribute__((constructor)) static void leaf_init(void) {
    ready = initialized == 17 && zeroed == 0;
    initialized = 23;
    write(1, "INIT leaf\n", 10);
}
int leaf_ready(void) { return ready; }
unsigned *leaf_initialized(void) { return &initialized; }
unsigned *leaf_zeroed(void) { return &zeroed; }
