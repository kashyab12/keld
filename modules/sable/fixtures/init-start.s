.text
.globl _start
.p2align 2
_start:
    bl _sable_main
    bl _exit

// Inspect the Darwin initializer arguments without assuming the native process's
// environment, executable pathname or other implementation-owned strings.
_check_args:
    stp x29, x30, [sp, #-16]!
    cmp x0, #1
    b.ne _bad_args
    ldr x5, [x1]
    cbz x5, _bad_args
    ldrb w6, [x5]
    cbz w6, _bad_args
    ldr x6, [x1, #8]
    cbnz x6, _bad_args
    cbz x2, _bad_args
    ldr x6, [x3]
    cbz x6, _bad_args
    ldr x6, [x4]
    ldr w7, [x6]
    mov w8, #0xfacf
    movk w8, #0xfeed, lsl #16
    cmp w7, w8
    b.ne _bad_args
    ldr x6, [x4, #8]
    ldr w7, [x6]
    cmp w7, w0
    b.ne _bad_args
    ldr x6, [x4, #16]
    ldr x6, [x6]
    cmp x6, x1
    b.ne _bad_args
    ldr x6, [x4, #24]
    ldr x6, [x6]
    cmp x6, x2
    b.ne _bad_args
    ldr x6, [x4, #32]
    ldr x6, [x6]
    cbz x6, _bad_args
    mov x0, #1
    adrp x1, _args_message@PAGE
    add x1, x1, _args_message@PAGEOFF
    mov x2, #5
    bl _write
    ldp x29, x30, [sp], #16
    ret
_bad_args:
    mov x0, #97
    bl _exit
.section __TEXT,__cstring,cstring_literals
_args_message: .asciz "ARGS\n"
.section __DATA,__mod_init_func,mod_init_funcs
.p2align 3
.quad _check_args
