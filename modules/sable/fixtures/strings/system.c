// Original scalar guest libc functions, not host calls or canned checks.
typedef unsigned long Word;
Word strlen(const char *s) { Word n=0; while ((unsigned char)s[n]) n++; return n; }
int strcmp(const char *a,const char *b) {
    const unsigned char *x=(const unsigned char *)a,*y=(const unsigned char *)b;
    while (*x && *x==*y) { x++; y++; }
    return (int)*x-(int)*y;
}
