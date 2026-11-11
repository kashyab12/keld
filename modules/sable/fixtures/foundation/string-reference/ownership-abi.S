// Original ARM64 scalar ABI caller; no platform x18 clobber and real frame/link baselines.
.text
.p2align 2
.globl _probe_claim_fallback
_probe_claim_fallback:
    b _objc_claimAutoreleasedReturnValue
.globl _probe_claim_candidate
_probe_claim_candidate:
    stp x29,x30,[sp,#-16]!
    mov x29,sp
    bl _probe_candidate_producer
    bl _objc_claimAutoreleasedReturnValue
    ldp x29,x30,[sp],#16
    ret
.globl _probe_release_x20
_probe_release_x20:
    stp x29,x30,[sp,#-112]!
    stp x19,x20,[sp,#16]
    stp x21,x22,[sp,#32]
    stp x23,x24,[sp,#48]
    stp x25,x26,[sp,#64]
    stp x27,x28,[sp,#80]
    str x0,[sp,#96]
    mov x29,sp
    mov x0, #1000
    mov x1, #1001
    mov x2, #1002
    mov x3, #1003
    mov x4, #1004
    mov x5, #1005
    mov x6, #1006
    mov x7, #1007
    mov x8, #1008
    mov x9, #1009
    mov x10, #1010
    mov x11, #1011
    mov x12, #1012
    mov x13, #1013
    mov x14, #1014
    mov x15, #1015
    mov x19, #1019
    mov x21, #1021
    mov x22, #1022
    mov x23, #1023
    mov x24, #1024
    mov x25, #1025
    mov x26, #1026
    mov x27, #1027
    mov x28, #1028
    ldr x20,[sp,#96]
    adrp x16,_release_before@PAGE
    add x16,x16,_release_before@PAGEOFF
    str x0,[x16,#0]
    str x1,[x16,#8]
    str x2,[x16,#16]
    str x3,[x16,#24]
    str x4,[x16,#32]
    str x5,[x16,#40]
    str x6,[x16,#48]
    str x7,[x16,#56]
    str x8,[x16,#64]
    str x9,[x16,#72]
    str x10,[x16,#80]
    str x11,[x16,#88]
    str x12,[x16,#96]
    str x13,[x16,#104]
    str x14,[x16,#112]
    str x15,[x16,#120]
    str x16,[x16,#128]
    str x17,[x16,#136]
    str x18,[x16,#144]
    str x19,[x16,#152]
    str x20,[x16,#160]
    str x21,[x16,#168]
    str x22,[x16,#176]
    str x23,[x16,#184]
    str x24,[x16,#192]
    str x25,[x16,#200]
    str x26,[x16,#208]
    str x27,[x16,#216]
    str x28,[x16,#224]
    str x29,[x16,#232]
    str x30,[x16,#240]
    adr x17,1f
    str x17,[x16,#240]
    mov x17,sp
    str x17,[x16,#248]
    bl _objc_release_x20
1:
    adrp x16,_release_after@PAGE
    add x16,x16,_release_after@PAGEOFF
    str x0,[x16,#0]
    str x1,[x16,#8]
    str x2,[x16,#16]
    str x3,[x16,#24]
    str x4,[x16,#32]
    str x5,[x16,#40]
    str x6,[x16,#48]
    str x7,[x16,#56]
    str x8,[x16,#64]
    str x9,[x16,#72]
    str x10,[x16,#80]
    str x11,[x16,#88]
    str x12,[x16,#96]
    str x13,[x16,#104]
    str x14,[x16,#112]
    str x15,[x16,#120]
    str x16,[x16,#128]
    str x17,[x16,#136]
    str x18,[x16,#144]
    str x19,[x16,#152]
    str x20,[x16,#160]
    str x21,[x16,#168]
    str x22,[x16,#176]
    str x23,[x16,#184]
    str x24,[x16,#192]
    str x25,[x16,#200]
    str x26,[x16,#208]
    str x27,[x16,#216]
    str x28,[x16,#224]
    str x29,[x16,#232]
    str x30,[x16,#240]
    mov x17,sp
    str x17,[x16,#248]
    ldp x19,x20,[sp,#16]
    ldp x21,x22,[sp,#32]
    ldp x23,x24,[sp,#48]
    ldp x25,x26,[sp,#64]
    ldp x27,x28,[sp,#80]
    ldp x29,x30,[sp],#112
    ret
.data
.p2align 3
.globl _release_before
_release_before: .space 256
.globl _release_after
_release_after: .space 256
