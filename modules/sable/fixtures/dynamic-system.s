// Original minimal Darwin ABI implementations; no Apple code or libraries.
.section __TEXT,__text,regular,pure_instructions
.globl _write
.globl _exit
.globl _getpid
.p2align 2
_write:
    mov x16, #4
    svc #0x80
    ret
_exit:
    mov x16, #1
    svc #0x80
    brk #1
_getpid:
    mov x16, #20
    svc #0x80
    ret
