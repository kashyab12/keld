extern long write(int, const void *, unsigned long);
extern void exit(int);
extern unsigned sable_leaf_ready(void);
static volatile unsigned state;
__attribute__((constructor)) static void initialize(void) {
    if (sable_leaf_ready() != 2 || state != 0) exit(93);
    state = 4;
    write(1, "INIT sibling\n", 13);
}
unsigned sable_sibling_ready(void) { return state; }
