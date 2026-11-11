// Original scalar ObjC bridge. No Apple implementation or caches.
.text
.p2align 2
.globl _objc_getClass
_objc_getClass:
    svc #0x5ab2
    ret
.globl _objc_msgSend
_objc_msgSend:
    cbz x0, 1f
    svc #0x5ab3
    br x16
1:  mov x0, #0
    ret
.data
.p2align 3
.globl __objc_empty_cache
__objc_empty_cache:
    .quad 0
