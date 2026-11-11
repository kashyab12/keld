// SPDX-License-Identifier: MIT
// Original Sable FP/SIMD calling-convention fixture. Headerless and freestanding:
// it declares its own CGRect-shaped structs and needs only write/exit.
// Every value is printed as raw IEEE bits so native and Sable output compare
// byte for byte, including NaN payloads and signed zeros.
extern long write(int, const void *, unsigned long);
extern void exit(int);

typedef double CGFloat;
typedef struct CGPoint { CGFloat x, y; } CGPoint;
typedef struct CGSize { CGFloat width, height; } CGSize;
typedef struct CGRect { CGPoint origin; CGSize size; } CGRect;
typedef struct Color4f { float r, g, b, a; } Color4f;
typedef unsigned long long u64;
typedef unsigned int u32;

// Clang may lower the zeroed CGRect array to memset; volatile keeps this byte loop scalar.
__attribute__((no_builtin("memset"))) void *memset(void *dst, int value, unsigned long n) {
    volatile unsigned char *p = dst;
    while (n--) *p++ = (unsigned char)value;
    return dst;
}

static char line[256];
static unsigned used;
static u64 digest = 5381;

static void put(const char *s) {
    while (*s) line[used++] = *s++;
}
// Only constant shifts: Sable's integer subset has no variable shifts or MUL.
static void hex(u64 v, unsigned digits) {
    static const char x[] = "0123456789abcdef";
    char text[16];
    for (unsigned i = digits; i-- > 0;) {
        text[i] = x[v & 15];
        v >>= 4;
    }
    for (unsigned i = 0; i < digits; i++) line[used++] = text[i];
}
static void flush(void) {
    line[used++] = '\n';
    for (unsigned i = 0; i < used; i++) digest = (digest << 5) + digest + (unsigned char)line[i];
    write(1, line, used);
    used = 0;
}
static u64 bits(double v) {
    union { double d; u64 u; } c = {v};
    return c.u;
}
static u32 fbits(float v) {
    union { float f; u32 u; } c = {v};
    return c.u;
}
static double from(u64 v) {
    union { u64 u; double d; } c = {v};
    return c.d;
}
static float ffrom(u32 v) {
    union { u32 u; float f; } c = {v};
    return c.f;
}
static void d(const char *name, double v) {
    put(name);
    put("=");
    hex(bits(v), 16);
    flush();
}
static void f(const char *name, float v) {
    put(name);
    put("=");
    hex(fbits(v), 8);
    flush();
}
static void q(const char *name, u64 v) {
    put(name);
    put("=");
    hex(v, 16);
    flush();
}
static void rect(const char *name, CGRect r) {
    put(name);
    put("=");
    hex(bits(r.origin.x), 16);
    put(",");
    hex(bits(r.origin.y), 16);
    put(",");
    hex(bits(r.size.width), 16);
    put(",");
    hex(bits(r.size.height), 16);
    flush();
}

// Volatile inputs keep the compiler from folding the IEEE edge cases.
static volatile u64 edge[] = {
    0x0000000000000000ull, 0x8000000000000000ull, 0x7ff0000000000000ull, 0xfff0000000000000ull,
    0x7ff8000000000001ull, 0x7ff0000000000002ull, 0x0000000000000001ull, 0x000fffffffffffffull,
    0x3ff0000000000000ull, 0xc00c000000000000ull, 0x7fefffffffffffffull, 0x3ff8000000000000ull,
};
static volatile u32 fedge[] = {0x00000001u, 0x7f800000u, 0x7fc00001u, 0x7f800002u, 0x3fc00000u, 0x80000000u};

__attribute__((noinline)) CGRect make_rect(CGFloat x, CGFloat y, CGFloat w, CGFloat h) {
    CGRect r = {{x, y}, {w, h}};
    return r;
}
__attribute__((noinline)) CGPoint center(CGRect r) {
    CGPoint p = {r.origin.x + r.size.width / 2, r.origin.y + r.size.height / 2};
    return p;
}
__attribute__((noinline)) CGRect inset(CGRect r, CGFloat dx, CGFloat dy) {
    return make_rect(r.origin.x + dx, r.origin.y + dy, r.size.width - 2 * dx, r.size.height - 2 * dy);
}
__attribute__((noinline)) CGRect union_rect(CGRect a, CGRect b) {
    CGFloat x0 = a.origin.x < b.origin.x ? a.origin.x : b.origin.x;
    CGFloat y0 = a.origin.y < b.origin.y ? a.origin.y : b.origin.y;
    CGFloat ax = a.origin.x + a.size.width, bx = b.origin.x + b.size.width;
    CGFloat ay = a.origin.y + a.size.height, by = b.origin.y + b.size.height;
    CGFloat x1 = ax > bx ? ax : bx;
    CGFloat y1 = ay > by ? ay : by;
    return make_rect(x0, y0, x1 - x0, y1 - y0);
}
__attribute__((noinline)) CGRect offset_by(int count, CGRect r, long scale, CGPoint delta, float bias) {
    return make_rect(r.origin.x + delta.x * count, r.origin.y + delta.y * scale, r.size.width + bias, r.size.height);
}
__attribute__((noinline)) unsigned contains(CGRect r, CGPoint p) {
    return p.x >= r.origin.x && p.y >= r.origin.y && p.x < r.origin.x + r.size.width &&
           p.y < r.origin.y + r.size.height;
}
__attribute__((noinline)) Color4f blend(Color4f a, Color4f b, float t) {
    Color4f c = {a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a};
    return c;
}
__attribute__((noinline)) CGSize integral(CGSize s) {
    CGSize r = {__builtin_ceil(s.width), __builtin_floor(s.height)};
    return r;
}

__attribute__((noinline)) CGRect layout(unsigned n) {
    CGRect cells[4] = {0};
    for (unsigned i = 0; i < n && i < 4; i++) cells[i] = make_rect(i * 10.0, i * -2.0, 8, 8 + i);
    CGRect total = cells[0];
    for (unsigned i = 1; i < 4; i++) total = union_rect(total, cells[i]);
    return total;
}
__attribute__((noinline)) double scale_u(unsigned u, short s, double k) {
    return u * k + s * k;
}

// Deliberately uses many FP temporaries so the callee clobbers V registers
// that the caller must not rely on, and saves the ones it must preserve.
__attribute__((noinline)) double churn(double seed) {
    double a = seed * 1.5, b = seed - 3.25, c = seed / 7.0, e = seed + 11.0;
    double g = a * b - c, h = b * c + e, i = c - e * a, j = e + a * b;
    double k = g * h + i, l = h - i * j, m = i * j + g, n = j - g * h;
    double o = k * l - m, p = l + m * n, r = m - n * k, s = n * k + l;
    q("churn.mark", bits(o + p + r + s));
    return (o - p) * (r + s) + (a + b + c + e) - (g * h * i * j) + (k - l + m - n);
}

// Eight doubles stay live across nested calls and an SVC, forcing d8-d15.
__attribute__((noinline)) double preserved(double seed) {
    double a = seed + 1, b = seed * 3, c = seed - 5, e = seed / 9;
    double g = a * b, h = c * e, i = a - c, j = b + e;
    double inner = churn(seed + 0.5);
    d("preserved.inner", inner);
    double again = churn(inner * 0.25);
    return a + b * 2 + c * 3 + e * 4 + g * 5 + h * 6 + i * 7 + j * 8 + again;
}

__attribute__((noinline)) void arithmetic_edges(void) {
    for (unsigned i = 0; i < sizeof edge / sizeof edge[0]; i++) {
        for (unsigned k = 0; k < sizeof edge / sizeof edge[0]; k += 3) {
            double x = from(edge[i]), y = from(edge[k]);
            put("edge.");
            hex(i, 2);
            hex(k, 2);
            put("=");
            hex(bits(x + y), 16);
            put(",");
            hex(bits(x - y), 16);
            put(",");
            hex(bits(x * y), 16);
            put(",");
            hex(bits(x / y), 16);
            put(",");
            hex(bits(__builtin_sqrt(x)), 16);
            put(",");
            hex(x < y ? 1u : x == y ? 2u : x > y ? 3u : 4u, 1);
            put(",");
            hex(bits(__builtin_fmin(x, y)), 16);
            put(",");
            hex(bits(__builtin_fmax(x, y)), 16);
            flush();
        }
    }
    for (unsigned i = 0; i < sizeof fedge / sizeof fedge[0]; i++) {
        float x = ffrom(fedge[i]), y = ffrom(fedge[i + 1 < 6 ? i + 1 : 0]);
        put("fedge.");
        hex(i, 2);
        put("=");
        hex(fbits(x + y), 8);
        put(",");
        hex(fbits(x * y), 8);
        put(",");
        hex(fbits(x / y), 8);
        put(",");
        hex(bits((double)x), 16);
        put(",");
        hex(fbits((float)from(edge[i * 2])), 8);
        flush();
    }
}

__attribute__((noinline)) void conversions(void) {
    static volatile double values[] = {-2.5, -0.5, 0.5, 1.5, 2.5, 3.999, -1e300, 1e300, 4294967296.0};
    for (unsigned i = 0; i < sizeof values / sizeof values[0]; i++) {
        double v = values[i];
        put("conv.");
        hex(i, 2);
        put("=");
        hex((u32)(int)v, 8);
        put(",");
        hex((u32)v, 8);
        put(",");
        hex((u64)(long long)v, 16);
        put(",");
        hex(bits(__builtin_floor(v)), 16);
        put(",");
        hex(bits(__builtin_ceil(v)), 16);
        put(",");
        hex(bits(__builtin_trunc(v)), 16);
        put(",");
        hex(bits(__builtin_fabs(-v)), 16);
        flush();
    }
    static volatile long long ints[] = {0, -1, 9007199254740993ll, -9223372036854775807ll - 1};
    static volatile unsigned long long uints[] = {0, 1, 18446744073709551615ull, 9007199254740993ull};
    for (unsigned i = 0; i < 4; i++) {
        put("cvt.");
        hex(i, 2);
        put("=");
        hex(bits((double)ints[i]), 16);
        put(",");
        hex(bits((double)uints[i]), 16);
        put(",");
        hex(fbits((float)ints[i]), 8);
        put(",");
        hex(fbits((float)(int)ints[i]), 8);
        put(",");
        hex(fbits((float)(unsigned)uints[i]), 8);
        flush();
    }
}

int main(void) {
    CGRect base = make_rect(10.5, -20.25, 300, 400.125);
    rect("make", base);
    CGPoint c = center(base);
    d("center.x", c.x);
    d("center.y", c.y);
    CGRect nested = inset(inset(base, 4, 8), center(base).x / 100, 0.75);
    rect("inset.nested", nested);
    rect("union", union_rect(nested, make_rect(-5, 3, 2, 1000)));
    CGPoint delta = {1.25, -2.5};
    rect("offset", offset_by(3, base, -4, delta, 0.5f));
    q("contains", (u64)contains(base, c) << 4 | (u64)contains(base, delta));
    Color4f red = {1, 0, 0, 1}, blue = {0, 0, 1, 1};
    Color4f mid = blend(red, blue, 0.25f);
    f("blend.r", mid.r);
    f("blend.b", mid.b);
    CGSize sz = integral(base.size);
    d("integral.w", sz.width);
    d("integral.h", sz.height);
    rect("layout.2", layout(2));
    rect("layout.4", layout(4));
    d("scale_u", scale_u(4000000000u, -7, 0.375));
    d("preserved", preserved(2.0));
    d("preserved.neg", preserved(-0.0));
    arithmetic_edges();
    conversions();
    q("digest", digest);
    put("HFA OK");
    flush();
    return 0;
}
