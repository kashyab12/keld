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
    mov x16, #4
    svc #0x80
    add sp, sp, #16
    mov x0, #0
    mov x16, #1
    svc #0x80
