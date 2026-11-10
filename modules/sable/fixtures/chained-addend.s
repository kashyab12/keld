.text
.globl _start
.p2align 2
_start:
    adrp x10, _positive@PAGE
    ldr x10, [x10, _positive@PAGEOFF]
    ldr x11, [x10]
    adrp x10, _negative@PAGE
    ldr x10, [x10, _negative@PAGEOFF]
    ldr x12, [x10]
    add x11, x11, x12
    adrp x10, _local_pointer@PAGE
    ldr x10, [x10, _local_pointer@PAGEOFF]
    ldr x12, [x10]
    add x11, x11, x12
    str x11, [sp, #-16]!
    mov x0, #1
    mov x1, sp
    mov x2, #8
    bl _write
    add sp, sp, #16
    mov x0, #0
    bl _exit
.data
.p2align 3
_positive: .quad _sable_numbers + 8
_negative: .quad _sable_numbers_end - 16
_local_pointer: .quad _local_value
_local_value: .quad 7
