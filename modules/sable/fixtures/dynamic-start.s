.section __TEXT,__text,regular,pure_instructions
.globl _start
.p2align 2
_start:
    bl _workload
    sub sp, sp, #16
    str x0, [sp]
    mov x0, #1
    mov x1, sp
    mov x2, #8
    adrp x16, _write@GOTPAGE
    ldr x16, [x16, _write@GOTPAGEOFF]
    blr x16
    add sp, sp, #16
    mov x0, #0
    adrp x16, _exit@GOTPAGE
    ldr x16, [x16, _exit@GOTPAGEOFF]
    br x16
