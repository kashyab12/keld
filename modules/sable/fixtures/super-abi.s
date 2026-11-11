// Reuse the original leaf capture IMP; C method wrappers must tail-call it.
.include "modules/sable/fixtures/objc-abi.s"
.text
.p2align 2
.globl _sable_abi_super
_sable_abi_super:
    stp x29,x30,[sp,#-48]!
    mov x29,sp
    stp x19,x20,[sp,#16]
    stp x0,x1,[sp,#32]
    mov x1,x2
    add x0,sp,#32
    adrp x19,_abi_capture@PAGE
    add x19,x19,_abi_capture@PAGEOFF
    str x18,[x19,#256]
    str x29,[x19,#264]
    mov x20,sp
    str x20,[x19,#272]
    adr x20,1f
    str x20,[x19,#280]
    mov x2,#102
    mov x3,#103
    mov x4,#104
    mov x5,#105
    mov x6,#106
    mov x7,#107
    mov x8,#108
    mov x9,#109
    mov x10,#110
    mov x11,#111
    mov x12,#112
    mov x13,#113
    mov x14,#114
    mov x15,#115
    bl _objc_msgSendSuper2
1:
    ldp x19,x20,[sp,#16]
    ldp x29,x30,[sp],#48
    ret
