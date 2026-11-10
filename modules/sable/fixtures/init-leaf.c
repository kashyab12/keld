extern long write(int, const void *, unsigned long);
extern void exit(int);
static volatile unsigned state;
__attribute__((constructor(101))) static void first(void) {
    if (state != 0) exit(90);
    state = 1;
    write(1, "INIT leaf 1\n", 12);
}
__attribute__((constructor(102))) static void second(void) {
    if (state != 1) exit(91);
    state = 2;
    write(1, "INIT leaf 2\n", 12);
}
unsigned sable_leaf_ready(void) { return state; }
