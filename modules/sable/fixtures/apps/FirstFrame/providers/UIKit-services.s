// SPDX-License-Identifier: MIT
// Original UI host-service stubs. Each SVC origin is checked by Sable.
.macro own name, number
.text
.p2align 2
.globl \name
\name:
    svc #\number
    ret
.endmacro
own _sable_ui_launch_info,0x5b50
own _sable_ui_present,0x5b51
own _sable_ui_wait_event,0x5b52
