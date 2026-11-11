// Original ABI probe; captures entry to the actual guest IMP before any prologue.
.text
.p2align 2
.globl _sable_abi_send
_sable_abi_send:
    stp x29, x30, [sp, #-32]!
    mov x29, sp
    stp x19, x20, [sp, #16]
    adrp x19, _abi_capture@PAGE
    add x19, x19, _abi_capture@PAGEOFF
    str x18, [x19, #256]
    str x29, [x19, #264]
    mov x20, sp
    str x20, [x19, #272]
    adr x20, 1f
    str x20, [x19, #280]
    mov x2, #102
    mov x3, #103
    mov x4, #104
    mov x5, #105
    mov x6, #106
    mov x7, #107
    mov x8, #108
    mov x9, #109
    mov x10, #110
    mov x11, #111
    mov x12, #112
    mov x13, #113
    mov x14, #114
    mov x15, #115
    bl _objc_msgSend
1:
    ldp x19, x20, [sp, #16]
    ldp x29, x30, [sp], #32
    ret
.globl _sable_abi_imp
_sable_abi_imp:
    adrp x16, _abi_capture@PAGE
    add x16, x16, _abi_capture@PAGEOFF
    str x0, [x16, #0]
    str x1, [x16, #8]
    str x2, [x16, #16]
    str x3, [x16, #24]
    str x4, [x16, #32]
    str x5, [x16, #40]
    str x6, [x16, #48]
    str x7, [x16, #56]
    str x8, [x16, #64]
    str x9, [x16, #72]
    str x10, [x16, #80]
    str x11, [x16, #88]
    str x12, [x16, #96]
    str x13, [x16, #104]
    str x14, [x16, #112]
    str x15, [x16, #120]
    str x18, [x16, #144]
    str x19, [x16, #152]
    str x20, [x16, #160]
    str x21, [x16, #168]
    str x22, [x16, #176]
    str x23, [x16, #184]
    str x24, [x16, #192]
    str x25, [x16, #200]
    str x26, [x16, #208]
    str x27, [x16, #216]
    str x28, [x16, #224]
    str x29, [x16, #232]
    str x30, [x16, #240]
    mov x17, sp
    str x17, [x16, #248]
    mov x0, #42
    ret
.data
.p2align 3
.globl _abi_capture
_abi_capture:
    .space 288
