// Original minimal resolver bridge, not an Apple/Darwin syscall implementation.
.include "modules/sable/fixtures/dynamic-system.s"
.globl __tlv_bootstrap
.p2align 2
__tlv_bootstrap:
    svc #0x5ab1
    ret
