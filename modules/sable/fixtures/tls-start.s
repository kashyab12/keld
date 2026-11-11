.text
.globl _start
.p2align 2
_start:
    bl _sable_main
    bl _exit
