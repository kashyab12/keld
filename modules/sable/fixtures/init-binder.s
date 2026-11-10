// The legacy fixture binds its lazy imports eagerly in Sable. A call to this
// unsupported binder must trap, never report successful lazy binding.
.text
.globl dyld_stub_binder
.p2align 2
dyld_stub_binder:
    brk #1
