// 10,000,000 iterations, 64-bit checksum. Arithmetic only: not a UI benchmark.
.section __TEXT,__text,regular,pure_instructions
.globl _start
.p2align 2
_start:
    movz x8, #0x9680
    movk x8, #0x98, lsl #16
    mov x9, #0
loop:
    add x9, x9, x8
    sub x8, x8, #1
    cbnz x8, loop
    mov x0, #0
    mov x16, #1
    svc #0x80
