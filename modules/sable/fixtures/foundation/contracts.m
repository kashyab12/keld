#import <Foundation/Foundation.h>
#include <mach-o/dyld.h>
#include <mach-o/loader.h>
#include <pthread.h>
#include <sys/mman.h>
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
extern int parent_ready(void);
extern unsigned *leaf_initialized(void);
extern unsigned *leaf_zeroed(void);
static int initialized;
static unsigned *main_address;
__attribute__((constructor)) static void executable_init(void) {
    initialized = parent_ready() && *leaf_initialized() == 23;
    write(1, "INIT executable\n", 16);
}
static void *worker(void *unused) {
    (void)unused;
    int ok;
    @autoreleasepool {
        unsigned *a = leaf_initialized(), *b = leaf_zeroed();
        ok = a != main_address && *a == 17 && *b == 0;
        *a = 41; *b = 9;
        ok = ok && leaf_initialized() == a && *leaf_initialized() == 41;
        ok = ok && [[@"thread" uppercaseString] isEqualToString:@"THREAD"];
    }
    return (void *)(uintptr_t)(ok ? 0 : 1);
}
static void metadata(void) {
    for (uint32_t i = 0; i < _dyld_image_count(); i++) {
        const char *path = _dyld_get_image_name(i);
        if (!strstr(path, "/Foundation.framework/") && !strstr(path, "/libobjc.A.dylib") && !strstr(path, "/libsystem_pthread.dylib")) continue;
        const struct mach_header_64 *mh = (const void *)_dyld_get_image_header(i);
        const struct load_command *lc = (const void *)(mh + 1);
        for (uint32_t n = 0; n < mh->ncmds; n++, lc = (const void *)((const char *)lc + lc->cmdsize)) {
            if (lc->cmd != LC_SEGMENT_64) continue;
            const struct segment_command_64 *seg = (const void *)lc;
            const struct section_64 *sect = (const void *)(seg + 1);
            for (uint32_t s = 0; s < seg->nsects; s++) {
                unsigned type = sect[s].flags & SECTION_TYPE;
                if (type == S_MOD_INIT_FUNC_POINTERS || type == S_INIT_FUNC_OFFSETS || (type >= S_THREAD_LOCAL_REGULAR && type <= S_THREAD_LOCAL_INIT_FUNCTION_POINTERS))
                    printf("META %s %.16s/%.16s type=%u size=%llu align=%u\n", path, sect[s].segname, sect[s].sectname, type, sect[s].size, sect[s].align);
            }
        }
    }
}
static int vm(void) {
    long page = sysconf(_SC_PAGESIZE);
    if (page <= 0) return 1;
    unsigned char *p = mmap(NULL, page * 2, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANON, -1, 0);
    if (p == MAP_FAILED) return 2;
    int ok = 1;
    for (long i = 0; i < page * 2; i++) if (p[i]) ok = 0;
    p[0] = 0x31; p[page] = 0x72;
    if (mprotect(p, page, PROT_READ)) ok = 0;
    if (p[0] != 0x31 || p[page] != 0x72) ok = 0;
    if (mprotect(p, page, PROT_READ | PROT_WRITE)) ok = 0;
    p[0] = 0x42;
    errno = 0;
    int bad = mprotect(p + 1, page, PROT_READ);
    int error = errno;
    if (bad != -1 || error != EINVAL) ok = 0;
    if (munmap(p, page * 2)) ok = 0;
    printf("VM page=%ld zero_fill=1 protect_restore=1 unaligned_return=%d errno=%d\n", page, bad, error);
    return ok ? 0 : 3;
}
int main(void) {
    if (!initialized) return 10;
    write(1, "MAIN\n", 5);
    @autoreleasepool {
        if (![[ @"sable foundation" uppercaseString] isEqualToString:@"SABLE FOUNDATION"]) return 11;
        main_address = leaf_initialized();
        if (*main_address != 23 || *leaf_zeroed() != 0) return 12;
        *leaf_zeroed() = 7;
        pthread_t thread;
        if (pthread_create(&thread, NULL, worker, NULL)) return 13;
        void *result;
        if (pthread_join(thread, &result) || result) return 14;
        if (*leaf_initialized() != 23 || *leaf_zeroed() != 7 || main_address != leaf_initialized()) return 15;
        printf("TLS ctor=23 worker_initial=17 worker_zero=0 main_after=23 main_zero_after=7 distinct=1 stable=1\n");
        if (vm()) return 16;
        metadata();
        puts("FOUNDATION CONTRACTS OK");
    }
    return 0;
}
