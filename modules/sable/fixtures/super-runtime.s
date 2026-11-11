// Original bounded Super2 bridge; checked pair load, private lookup, guest tail call.
.include "modules/sable/fixtures/protocol-runtime.s"
.text
.p2align 2
.globl _objc_msgSendSuper2
_objc_msgSendSuper2:
    ldp x0,x17,[x0]
    svc #0x5ab7
    br x16
