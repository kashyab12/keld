# Checked image initialization

Sable's explicit-library linker now accepts `S_INIT_FUNC_OFFSETS` (32-bit image
offsets) and `S_MOD_INIT_FUNC_POINTERS` (64-bit pointers after supported fixups).
The strict freestanding parser still requires the dynamic path for these arrays.
Arrays are bounded, aligned and file-backed; offset storage must be read-only.
The linker checks every target against the owning image's executable ranges
before returning a linked image. Initializer dependency cycles are rejected.

The eager plan follows declared dependencies in order, children before parents,
and visits a shared image once. Entries retain section/array order. The runtime
runs that plan before main with fresh private writable memory and stack on every
prepared run. Image preparation retains no completed-initializer state. All guest
instructions, including callback returns, consume the same process budget.
Callback exit/trap/exhaustion prevents main; unbalanced SP on return is an error.

Each callback receives guest-owned argc/argv/envp/apple/ProgramVars data. The
bounded default profile has argc=1, argv[0]=`sable`, empty environment and an
`executable_path=sable` apple string. ProgramVars contains the main header and
guest pointers to the argc/argv/environ/progname cells. Callbacks receive x0–x4,
a 16-byte-aligned stack and a guest return sentinel. Main receives the same
profile; a normal main return becomes its exit status. Images without an
initializer plan retain the existing freestanding entry behavior.

The return sentinel is outside executable guest ranges and never represents a
host address. Native execution recognizes it only while a checked guest callback
or startup-profile main call is active. A return that consumes the final budgeted
instruction succeeds; an ordinary invalid-PC jump still faults. Scalar registers
reset between startup calls; writable process memory remains shared within that
run. This does not implement signals, threads or reentrant loader entry.

The ABI/layout references are Apple's
[initializer enumeration](https://github.com/apple-oss-distributions/dyld/blob/fd8d0c4d52320ebf64db34f3cb280310d905c5ae/common/MachOAnalyzer.cpp#L2333),
[loader calls](https://github.com/apple-oss-distributions/dyld/blob/fd8d0c4d52320ebf64db34f3cb280310d905c5ae/dyld/Loader.cpp#L2590) and
[ProgramVars](https://github.com/apple-oss-distributions/dyld/blob/fd8d0c4d52320ebf64db34f3cb280310d905c5ae/dyld/DyldProcessConfig.h#L83).
The implementation is original Rust.

## Evidence and limits

Original C/assembly fixtures form a diamond with two callbacks in the shared
leaf, then parent, sibling, executable callback, argument inspection and main.
Both modern offset arrays and legacy pointer arrays produce the same checked
order on native Apple Simulator and Sable. The scalar fixtures use volatile
status words so their per-process memory mutations remain observable. The legacy
fixture's unsupported lazy binder traps if called; Sable binds eagerly and does
not call it. Native Apple execution uses its installed real LibSystem/binder.

`bash scripts/check-initializers.sh` rebuilds both formats using general LLVM,
runs every available engine and compares exact output. Integration tests cover
shared-leaf once-only scheduling, supplied-library reordering, three repeated
prepared runs, full interpreter/JIT outcomes with both indirect dispatch modes,
malformed arrays/targets, callback exit/loop, SP imbalance and exact budget edges.
The [execution report](../evidence/initializers/RESULTS.md) retains native/cloud
receipts. The [Foundation probes](foundation-contracts.md) explain why this
startup contract was prioritized.

TLS and terminators still reject explicitly. Objective-C `+load`, registration,
C++ destructors, delayed/shared-cache initialization, LC_ROUTINES_64, dlopen and
reentrant initialization are outside this gate. A read-only startup plan does
not model guest mutation of initializer metadata between callbacks. This change
supplies neither Foundation/UIKit nor graphics or input services.
