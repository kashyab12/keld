# Image initialization runs before main

Observed 2026-11-10. Sable now executes checked C initializer arrays before the
program entry. This is a bounded Darwin startup contract. It does not supply
Foundation/UIKit, TLS, threads, graphics or an app-performance result.

The new original program forms a diamond: parent and sibling share a leaf. The
leaf has two callbacks; main has an ordinary C callback and an assembly callback
that checks startup arguments. Every callback checks earlier data mutations.
Both offset arrays with chained fixups and pointer arrays with legacy fixups
produce exactly:

```text
INIT leaf 1
INIT leaf 2
INIT parent
INIT sibling
INIT executable
ARGS
MAIN
```

The shared leaf appears once. `ARGS` checks argc=1, argv termination, readable
argument strings, apple and ProgramVars, including the Mach-O header magic and
argc/argv/envp indirections. The assembly does not inspect or print native
environment values. Sable supplies its documented fixed guest launch profile.

## Independent and Linux execution

The [Apple-native receipt](apple-native.json) records both initializer programs
and four regression programs passing on installed iOS Simulator 27.2 build
24B8117. Its private device set was deleted and listed zero devices. Native Apple
execution resolves the real installed LibSystem. The original minimal system
library used on Linux implements only its stated ABI subset. The legacy
fixture's unused lazy binder traps if invoked; Sable resolves bindings eagerly.

The [AWS run](aws-01/run.json) used a disposable Ubuntu 24.04 c7i.large, x86-64
Xeon Platinum 8488C and Rust 1.93.1. Remote exit was 0. **38 Rust tests passed**.
Both rebuilt formats produced the native-checked stream in the interpreter and
native translator. See [four raw streams](aws-01/results/initializers),
[stdout](aws-01/stdout.txt), [stderr](aws-01/stderr.txt),
[runtime source manifest](aws-01/runtime-source-sha256.json) and
[original fixture hashes](fixture-sha256.json). The runtime/code/fixture hashes
in the AWS manifest were checked against the integrated checkout.

Tests additionally check three prepared repeats, identical full outcomes under
both native indirect-dispatch modes, reversed supplied-library order, invalid
arrays and non-code targets. Synthetic execution checks cover a callback that
exits, a nonreturning callback, SP imbalance, a return on the exact budget edge
and propagation of main's returned exit status. Initialization and main share
one instruction budget and per-run writable memory. Native sentinel handling
is active only for checked startup calls; ordinary invalid-PC behavior remains
covered by the previous regression suite.

Local recipe checks passed with **32 Mac Rust tests**, independent scalar oracle
validation, and rebuilding/executing the original legacy, chained and initializer
programs. No new guest ISA instruction was introduced. The fixture status words
are volatile so their actual memory mutations remain observable within the
existing scalar subset.

The controller verified [zero remaining AWS instances, volumes, security groups
and keys](aws-01/cleanup.json). The [host graphics inventory](aws-01/results/graphics-inventory.json)
is retained; no renderer or app interaction ran on that host. Apple SDK/runtime
binaries and the SDK-linked research probes were never transferred there.
The arithmetic timing also printed by the standard smoke script is excluded
from app-performance conclusions.

## Connection to the Foundation investigation

The reviewed [original Foundation research](../../docs/foundation-contracts.md)
observed native constructor order, C TLS isolation and explicit VM behavior.
Its [original receipt](../foundation/receipt.json) and
[integrated runner repeat](../foundation/integration-run.json) each record four
native passes: minimal Foundation and the companion on macOS ARM64 and iOS
Simulator ARM64. Both controllers verified owned-device cleanup. Original
sources and a local-only reproduction runner are published; Apple build outputs
remain private.

Only the first startup gate was implemented here. TLS sections, terminators,
Objective-C image registration/+load, C++ destructors, cyclic/delayed/reentrant
initialization, shared-cache loading and general process launch arguments remain
outside the implemented profile. Initializer targets are fixed in a validated
plan before execution; guest mutation of initializer arrays is not modeled.
The real Foundation/UIKit dependency and app/GPU gates remain open. See the
[startup contract](../../docs/initializers.md) and
[app benchmark requirements](../../docs/app-benchmarks.md).
