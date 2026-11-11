__thread unsigned long a_initialized = 17;
__thread unsigned long a_zeroed;
unsigned long *a_initialized_ptr(void) { return &a_initialized; }
unsigned long *a_zeroed_ptr(void) { return &a_zeroed; }
