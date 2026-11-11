// The release argument is x20; x0 is an unrelated invalid object sentinel.
.text
.p2align 2
.globl _release_x20_control
_release_x20_control:
    stp x20,x30,[sp,#-16]!
    mov x20,x0
    mov x0,#123
    bl _objc_release_x20
    ldp x20,x30,[sp],#16
    ret
