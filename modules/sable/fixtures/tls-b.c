__thread unsigned long b_initialized = 29;
__thread unsigned long b_zeroed;
unsigned long *b_initialized_ptr(void) { return &b_initialized; }
unsigned long *b_zeroed_ptr(void) { return &b_zeroed; }
