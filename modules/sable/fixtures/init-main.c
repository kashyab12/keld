extern long write(int, const void *, unsigned long);
extern void exit(int);
extern unsigned sable_leaf_ready(void), sable_parent_ready(void), sable_sibling_ready(void);
static volatile unsigned state;
__attribute__((constructor)) static void initialize(void) {
    if (sable_leaf_ready() != 2 || sable_parent_ready() != 3 || sable_sibling_ready() != 4 || state != 0) exit(94);
    state = 5;
    write(1, "INIT executable\n", 16);
}
int sable_main(void) {
    if (state != 5 || sable_leaf_ready() != 2 || sable_parent_ready() != 3 || sable_sibling_ready() != 4) return 95;
    write(1, "MAIN\n", 5);
    return 0;
}
