// SPDX-License-Identifier: MIT
// Original byte-wise libc helpers clang may call for struct copies.
typedef unsigned long Word;
__attribute__((no_builtin("memset"))) void *memset(void *dst, int value, Word n) {
    volatile unsigned char *p = dst;
    while (n--) *p++ = (unsigned char)value;
    return dst;
}
__attribute__((no_builtin("memcpy"))) void *memcpy(void *dst, const void *src, Word n) {
    volatile unsigned char *d = dst;
    const unsigned char *s = src;
    while (n--) *d++ = *s++;
    return dst;
}
