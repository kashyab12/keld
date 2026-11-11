// SPDX-License-Identifier: MIT
// Original libobjc service stubs for the UI profile: checked SVC bridges only.
.include "modules/sable/fixtures/instance-runtime.s"
.macro own name, number
.text
.p2align 2
.globl \name
\name:
    svc #\number
    ret
.endmacro
own _objc_autoreleasePoolPush,0x5ac0
own _objc_autoreleasePoolPop,0x5ac1
own _objc_retain,0x5ac2
own _objc_release,0x5ac3
own _objc_autorelease,0x5ac4
own _objc_retainAutorelease,0x5ac5
own _objc_claimAutoreleasedReturnValue,0x5ac6
own _objc_retainAutoreleasedReturnValue,0x5ac7
own _objc_release_x20,0x5ac8
own _objc_release_x21,0x5ac9
own _objc_release_x22,0x5aca
own _objc_release_x23,0x5acb
own _objc_release_x24,0x5acc
own _objc_retain_x22,0x5acd
// Register-specific bridges for every other x0-x28 (one generated table).
.irp n, 0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,25,26,27,28
own _objc_release_x\n,(0x5b00+\n)
.endr
.irp n, 0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,23,24,25,26,27,28
own _objc_retain_x\n,(0x5b20+\n)
.endr
// Reflection services used by the original NSObject/Foundation/UIKit providers.
own _class_respondsToSelector,0x5b40
own _class_getName,0x5b41
own _sable_class_cxx_destruct,0x5b42
