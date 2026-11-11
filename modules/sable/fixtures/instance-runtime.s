// Original checked scalar creation/disposal services; no host object pointers.
.include "modules/sable/fixtures/super-runtime.s"
.text
.p2align 2
.globl _class_createInstance
_class_createInstance:
    svc #0x5ab8
    ret
.globl _object_dispose
_object_dispose:
    svc #0x5ab9
    ret
