// Original protocol service boundary; no Apple implementation.
.include "modules/sable/fixtures/objc-runtime.s"
.text
.p2align 2
.globl _objc_getProtocol
_objc_getProtocol:
    svc #0x5ab4
    ret
.globl _protocol_getName
_protocol_getName:
    svc #0x5ab5
    ret
.globl _class_conformsToProtocol
_class_conformsToProtocol:
    svc #0x5ab6
    ret
.section __DATA_CONST,__const
.globl _sable_protocol_nil
_sable_protocol_nil:
    .asciz "nil"
