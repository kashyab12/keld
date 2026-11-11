// Original Darwin ARM64 TLV ABI oracle. No Apple implementation included.
.text
.p2align 2
.globl _probe_a
_probe_a:
    adrp x1, _a_initialized@TLVPPAGE
    ldr x1, [x1, _a_initialized@TLVPPAGEOFF]
    b Lcommon
.globl _probe_b
_probe_b:
    adrp x1, _b_initialized@TLVPPAGE
    ldr x1, [x1, _b_initialized@TLVPPAGEOFF]
    b Lcommon
Lcommon:
    sub sp, sp, #816
    stp x0, x1, [sp, #0]
    stp x2, x3, [sp, #16]
    stp x4, x5, [sp, #32]
    stp x6, x7, [sp, #48]
    stp x8, x9, [sp, #64]
    stp x10, x11, [sp, #80]
    stp x12, x13, [sp, #96]
    stp x14, x15, [sp, #112]
    stp x16, x17, [sp, #128]
    stp x18, x19, [sp, #144]
    stp x20, x21, [sp, #160]
    stp x22, x23, [sp, #176]
    stp x24, x25, [sp, #192]
    stp x26, x27, [sp, #208]
    stp x28, x29, [sp, #224]
    str x30, [sp, #240]
#if PROBE_VECTOR
    stp q0, q1, [sp, #256]
    stp q2, q3, [sp, #288]
    stp q4, q5, [sp, #320]
    stp q6, q7, [sp, #352]
    stp q8, q9, [sp, #384]
    stp q10, q11, [sp, #416]
    stp q12, q13, [sp, #448]
    stp q14, q15, [sp, #480]
    stp q16, q17, [sp, #512]
    stp q18, q19, [sp, #544]
    stp q20, q21, [sp, #576]
    stp q22, q23, [sp, #608]
    stp q24, q25, [sp, #640]
    stp q26, q27, [sp, #672]
    stp q28, q29, [sp, #704]
    stp q30, q31, [sp, #736]
#endif
    mov x1, #257
    mov x2, #258
    mov x3, #259
    mov x4, #260
    mov x5, #261
    mov x6, #262
    mov x7, #263
    mov x8, #264
    mov x9, #265
    mov x10, #266
    mov x11, #267
    mov x12, #268
    mov x13, #269
    mov x14, #270
    mov x15, #271
    mov x19, #275
    mov x20, #276
    mov x21, #277
    mov x22, #278
    mov x23, #279
    mov x24, #280
    mov x25, #281
    mov x26, #282
    mov x27, #283
    mov x28, #284
    ldr x16, [sp, #0]
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
    adr x17, Lresume
    str x17, [x16, #240]
    mov x17, sp
    str x17, [x16, #504]
#if PROBE_VECTOR
    adrp x17, Lpatterns@PAGE
    add x17, x17, Lpatterns@PAGEOFF
    ldp q0, q1, [x17, #0]
    ldp q2, q3, [x17, #32]
    ldp q4, q5, [x17, #64]
    ldp q6, q7, [x17, #96]
    ldp q8, q9, [x17, #128]
    ldp q10, q11, [x17, #160]
    ldp q12, q13, [x17, #192]
    ldp q14, q15, [x17, #224]
    ldp q16, q17, [x17, #256]
    ldp q18, q19, [x17, #288]
    ldp q20, q21, [x17, #320]
    ldp q22, q23, [x17, #352]
    ldp q24, q25, [x17, #384]
    ldp q26, q27, [x17, #416]
    ldp q28, q29, [x17, #448]
    ldp q30, q31, [x17, #480]
    add x17, x16, #528
    stp q0, q1, [x17, #0]
    stp q2, q3, [x17, #32]
    stp q4, q5, [x17, #64]
    stp q6, q7, [x17, #96]
    stp q8, q9, [x17, #128]
    stp q10, q11, [x17, #160]
    stp q12, q13, [x17, #192]
    stp q14, q15, [x17, #224]
    stp q16, q17, [x17, #256]
    stp q18, q19, [x17, #288]
    stp q20, q21, [x17, #320]
    stp q22, q23, [x17, #352]
    stp q24, q25, [x17, #384]
    stp q26, q27, [x17, #416]
    stp q28, q29, [x17, #448]
    stp q30, q31, [x17, #480]
#endif
    ldr x0, [sp, #8]
    ldr x16, [x0]
    blr x16
Lresume:
    ldr x16, [sp, #0]
    str x0, [x16, #496]
    str x1, [x16, #256]
    str x2, [x16, #264]
    str x3, [x16, #272]
    str x4, [x16, #280]
    str x5, [x16, #288]
    str x6, [x16, #296]
    str x7, [x16, #304]
    str x8, [x16, #312]
    str x9, [x16, #320]
    str x10, [x16, #328]
    str x11, [x16, #336]
    str x12, [x16, #344]
    str x13, [x16, #352]
    str x14, [x16, #360]
    str x15, [x16, #368]
    str x18, [x16, #392]
    str x19, [x16, #400]
    str x20, [x16, #408]
    str x21, [x16, #416]
    str x22, [x16, #424]
    str x23, [x16, #432]
    str x24, [x16, #440]
    str x25, [x16, #448]
    str x26, [x16, #456]
    str x27, [x16, #464]
    str x28, [x16, #472]
    str x29, [x16, #480]
    str x30, [x16, #488]
    mov x17, sp
    str x17, [x16, #512]
#if PROBE_VECTOR
    add x17, x16, #1040
    stp q0, q1, [x17, #0]
    stp q2, q3, [x17, #32]
    stp q4, q5, [x17, #64]
    stp q6, q7, [x17, #96]
    stp q8, q9, [x17, #128]
    stp q10, q11, [x17, #160]
    stp q12, q13, [x17, #192]
    stp q14, q15, [x17, #224]
    stp q16, q17, [x17, #256]
    stp q18, q19, [x17, #288]
    stp q20, q21, [x17, #320]
    stp q22, q23, [x17, #352]
    stp q24, q25, [x17, #384]
    stp q26, q27, [x17, #416]
    stp q28, q29, [x17, #448]
    stp q30, q31, [x17, #480]
#endif
    ldr x18, [sp, #144]
#if PROBE_VECTOR
    ldp q0, q1, [sp, #256]
    ldp q2, q3, [sp, #288]
    ldp q4, q5, [sp, #320]
    ldp q6, q7, [sp, #352]
    ldp q8, q9, [sp, #384]
    ldp q10, q11, [sp, #416]
    ldp q12, q13, [sp, #448]
    ldp q14, q15, [sp, #480]
    ldp q16, q17, [sp, #512]
    ldp q18, q19, [sp, #544]
    ldp q20, q21, [sp, #576]
    ldp q22, q23, [sp, #608]
    ldp q24, q25, [sp, #640]
    ldp q26, q27, [sp, #672]
    ldp q28, q29, [sp, #704]
    ldp q30, q31, [sp, #736]
#endif
    ldp x0, x1, [sp, #0]
    ldp x2, x3, [sp, #16]
    ldp x4, x5, [sp, #32]
    ldp x6, x7, [sp, #48]
    ldp x8, x9, [sp, #64]
    ldp x10, x11, [sp, #80]
    ldp x12, x13, [sp, #96]
    ldp x14, x15, [sp, #112]
    ldp x16, x17, [sp, #128]
    ldp x18, x19, [sp, #144]
    ldp x20, x21, [sp, #160]
    ldp x22, x23, [sp, #176]
    ldp x24, x25, [sp, #192]
    ldp x26, x27, [sp, #208]
    ldp x28, x29, [sp, #224]
    ldr x30, [sp, #240]
    add sp, sp, #816
    ret
#if PROBE_VECTOR
.section __TEXT,__const
.p2align 4
Lpatterns:
    .quad 1161981756646125568, 9264081114510712832
    .quad 1161981756646125569, 9264081114510712833
    .quad 1161981756646125570, 9264081114510712834
    .quad 1161981756646125571, 9264081114510712835
    .quad 1161981756646125572, 9264081114510712836
    .quad 1161981756646125573, 9264081114510712837
    .quad 1161981756646125574, 9264081114510712838
    .quad 1161981756646125575, 9264081114510712839
    .quad 1161981756646125576, 9264081114510712840
    .quad 1161981756646125577, 9264081114510712841
    .quad 1161981756646125578, 9264081114510712842
    .quad 1161981756646125579, 9264081114510712843
    .quad 1161981756646125580, 9264081114510712844
    .quad 1161981756646125581, 9264081114510712845
    .quad 1161981756646125582, 9264081114510712846
    .quad 1161981756646125583, 9264081114510712847
    .quad 1161981756646125584, 9264081114510712848
    .quad 1161981756646125585, 9264081114510712849
    .quad 1161981756646125586, 9264081114510712850
    .quad 1161981756646125587, 9264081114510712851
    .quad 1161981756646125588, 9264081114510712852
    .quad 1161981756646125589, 9264081114510712853
    .quad 1161981756646125590, 9264081114510712854
    .quad 1161981756646125591, 9264081114510712855
    .quad 1161981756646125592, 9264081114510712856
    .quad 1161981756646125593, 9264081114510712857
    .quad 1161981756646125594, 9264081114510712858
    .quad 1161981756646125595, 9264081114510712859
    .quad 1161981756646125596, 9264081114510712860
    .quad 1161981756646125597, 9264081114510712861
    .quad 1161981756646125598, 9264081114510712862
    .quad 1161981756646125599, 9264081114510712863
#endif
