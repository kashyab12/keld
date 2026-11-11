// Original freestanding scalar TLV regression; no SDK or libc formatting.
typedef unsigned long Word;
struct observation { Word expected[31], actual[31], result, expected_sp, actual_sp, padding; };
extern void probe_a(struct observation *), probe_b(struct observation *);
extern Word *a_initialized_ptr(void), *a_zeroed_ptr(void), *b_initialized_ptr(void), *b_zeroed_ptr(void);
extern long write(int, const void *, unsigned long);
static int constructor_ok;
static Word *capture(void (*probe)(struct observation *)) {
    struct observation o;
    probe(&o);
    for (unsigned r=1;r<=30;r++) {
        if (r==16 || r==17) continue;
        if (o.expected[r]!=o.actual[r]) return (void *)0;
    }
    if (o.expected_sp!=o.actual_sp || (o.actual_sp&15)) return (void *)0;
    return (Word *)o.result;
}
#if TLS_CTOR
__attribute__((constructor)) static void before_main(void) {
    Word *a=capture(probe_a);
    if (!a || *a!=17 || *a_zeroed_ptr()!=0) return;
    *a=23;*a_zeroed_ptr()=7;constructor_ok=1;
}
#endif
int sable_main(void) {
    Word *a=capture(probe_a), *again, *b;
    if (!a || a!=a_initialized_ptr()) return 11;
#if TLS_CTOR
    if (!constructor_ok || *a!=23 || *a_zeroed_ptr()!=7) return 12;
#else
    if (*a!=17 || *a_zeroed_ptr()!=0) return 13;
#endif
    *a=23;*a_zeroed_ptr()=7;
    again=capture(probe_a);
    if (again!=a || *again!=23 || *a_zeroed_ptr()!=7) return 14;
    b=capture(probe_b);
    if (!b || *b!=29 || *b_zeroed_ptr()!=0 || b!=b_initialized_ptr()) return 15;
    if (a==b || a_zeroed_ptr()==b_zeroed_ptr()) return 16;
    *b=41;*b_zeroed_ptr()=9;
    again=capture(probe_b);
    if (again!=b || *again!=41 || *a!=23 || *a_zeroed_ptr()!=7 || *b_zeroed_ptr()!=9) return 17;
    static const char ok[]="TLV scalar ABI, templates, two images, stable addresses OK\n";
    write(1,ok,sizeof(ok)-1);
    return 0;
}
