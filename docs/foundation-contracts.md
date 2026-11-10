# Foundation startup contracts

Observed 2026-11-10 against Sable main `5170468922d26a0b2058daacd786eb549bb3c40f`.
Implement checked image initialization first, then plain C Mach-O thread-local
variables for the existing scalar execution subset. These are bounded startup
steps; neither supplies Foundation, Objective-C, pthread scheduling or graphics.

Follow-up: the bounded C initializer gate is now implemented in
[Sable startup](initializers.md), with independently checked original fixtures.
TLS remains unsupported. The proposed gates and native observations below
record the investigation that led to this work.

The original research ran in a private artifact directory. Its source and curated receipts have been reviewed and integrated here; Apple build products remain private.

## Native observations

The original [minimal program](../modules/sable/fixtures/foundation/minimal.m) creates an autorelease pool,
uppercases an NSString, checks its UTF-8 contents, writes `SABLE FOUNDATION`, drains
the pool and returns. The separate [companion](../modules/sable/fixtures/foundation/contracts.m) adds two
original C dylibs, constructor checks, a pthread using Foundation and C TLS, and
explicit VM calls. Separating them prevents companion behavior becoming an
unsupported claim about minimal Foundation's internal dependencies.

Both executables built with scoped `DEVELOPER_DIR`, Xcode 27.2 build 27B52,
`-O1`, ARC and the installed SDK. Each passed once on native macOS ARM64
26.6.2 build 25G83 and once on iOS Simulator ARM64 27.2 build 24B8117.
The simulator deployment target was iOS 17. The checks used Apple's installed
runtime locally. No Apple binary, SDK, test runner or credential was exported.

| Check | Observed result on both native paths |
| --- | --- |
| Minimal Foundation | Exit 0; exactly `SABLE FOUNDATION\n` |
| Original constructor sequence | `INIT leaf`, `INIT parent`, `INIT executable`, `MAIN`, each once |
| Main-thread TLS | Template value 17 / zero value 0; leaf constructor changes initialized value to 23; main sees 23 |
| Worker-thread TLS and Foundation | Fresh 17 / 0; addresses differ from the live main thread; writes 41 / 9; repeated access stable; NSString uppercase check passes |
| Main after join | Still 23 / 7; original address unchanged |
| Explicit VM | Page size 16,384; two-page private anonymous map zero-filled; writes survive read-only/restore protection; unaligned mprotect returns -1, errno 22; munmap returns 0 |

The VM probe restores write permission before writing again. It does not attempt
a forbidden write, inspect post-unmap memory, exercise file-backed mappings or
trace Foundation's VM syscalls. Printed success fields are backed by program
checks and exit 0 in this receipt; they are not an independent syscall trace.

The controller created only private set `foundation-private-set-in_m8geo`, UUID
`6991D60C-6693-4B6C-8321-4C17EC94E7A6`. Its `finally` block shut down and deleted
that UUID. Shutdown/delete returned 0; listing that exact set found **zero devices**
and no owned UUID. No existing simulator was targeted. Source, SDK outputs and
original binaries remain private local research artifacts; owned devices are gone.
See [receipt](../evidence/foundation/receipt.json), [original metadata](../evidence/foundation/original-metadata.json)
and [startup summary](../evidence/foundation/startup-summary.json).

The native controller used the exact runtime build 24B8117. The original source is published for local SDK builds; its four native observations and source hashes remain in the linked receipt.

## Imports, metadata and execution are different evidence

The minimal simulator executable directly declares Foundation, libobjc,
LibSystem and CoreFoundation dependencies. Its ten direct imports are the CF
constant-string class reference, autorelease-pool push/pop, message send,
autoreleased-result claim, specialized release, retain/autorelease, strcmp,
strlen and write. Source execution and checked output exercise the NSString
operations, pool scope and write; the import list alone does not prove every
imported entry point ran. The original executable contains no constructor or TLS
sections. It has chained fixups, but Sable never reaches their application here.

With `DYLD_PRINT_INITIALIZERS=1`, the minimal simulator process reports **377
initializer calls** across its lifetime: LibSystem, libsystem_trace, libc++abi,
libc++, CoreFoundation, Swift core/concurrency and Network. Network accounts for
365. This establishes initializer execution during this process, not use of
network APIs, creation of sockets or 377 pre-main callbacks. Separate stdout and
stderr logs do not establish a global ordering. Apple's delayed initialization
and shared-cache behavior are outside the proposed first gate.

The original leaf dylib has two 24-byte descriptors in `__thread_vars` (type
0x13), a four-byte initialized template (0x11) and four-byte zero-fill section
(0x12). On disk, descriptor keys are zero and offsets are 0 and 4. It imports
`__tlv_bootstrap` (Mach-O symbol spelling, including the leading underscore).
The constructor's TLS use and checked worker/main values demonstrate those
original variables' runtime behavior. They do not identify Foundation's private
TLS use. The executable and both original dylibs each have a four-byte
`__TEXT,__init_offsets` section (type 0x16), so an implementation limited to old
`__mod_init_func` pointers would still reject this compiler's output.

The existing Sable release binary was run read-only on both exact original
simulator executables. Its hash is in the startup summary:

- Minimal: exit 125, `required dylib not supplied: /System/Library/Frameworks/Foundation.framework/Foundation`.
- Companion: exit 125, `initializers/terminators/TLS sections unsupported`.

Neither executes guest Foundation instructions. Supplying Apple's frameworks was
not attempted. Main's original chained-fixup fixtures already address the earlier
format rejection. The actual Notes app now stops at missing UIKit before its own
pointer patching or startup, as recorded in `evidence/chained/RESULTS.md`.

## Gate 1: checked per-process image initialization

Accept `S_INIT_FUNC_OFFSETS` and `S_MOD_INIT_FUNC_POINTERS` with checked metadata.
Offsets are unsigned 32-bit values relative to the owning image's load address;
pointer entries are 64-bit addresses after supported fixups. Require the offset
section to be read-only and four-byte aligned/sized, and pointer arrays to be
eight-byte aligned/sized. Every target must be aligned and executable in the
owning image. Reject truncated/out-of-range arrays and arithmetic overflow before
executing any initializer. These rules follow Apple's
[initializer enumeration and validation](https://github.com/apple-oss-distributions/dyld/blob/fd8d0c4d52320ebf64db34f3cb280310d905c5ae/common/MachOAnalyzer.cpp#L2333).

For an ordinary eager dependency graph, complete linking and required runtime
setup, then walk dependency declarations in order, children before parent.
Visit each image once, call entries in section/array order, and enter the main
program only after its callbacks return. Keep initialization state per process
run; preparing or reusing an immutable Image must never retain completed
initializers or modified data. Use the same checked guest calls, private memory,
stack and total instruction budget as main. A callback exit, trap or exhausted
budget ends that run; main must not start. Restrict the first gate to ordinary
acyclic dependencies and explicit rejection of unsupported initialization modes.
Apple's [loader traversal](https://github.com/apple-oss-distributions/dyld/blob/fd8d0c4d52320ebf64db34f3cb280310d905c5ae/dyld/Loader.cpp#L2590)
also performs Objective-C initialization before C callbacks; accepting C callbacks
does not cover `+load` or ObjC image registration.

Darwin passes argc, argv, envp, apple and a ProgramVars pointer to initializers.
Any accepted startup-argument support must use guest-owned arrays/strings and
ProgramVars fields, never host addresses. First-party assembly should inspect
these arguments before declaring that ABI implemented. Definition and call site:
[process startup types](https://github.com/apple-oss-distributions/dyld/blob/fd8d0c4d52320ebf64db34f3cb280310d905c5ae/dyld/DyldProcessConfig.h#L91)
and the loader above. Keep LC_ROUTINES_64, terminators, C++ destructors, upward
links, dlopen/reentrancy and delayed/cache initialization explicit unsupported
cases until separately tested.

Bounded first-party regression set, to implement with this gate:

1. A TLS-free leaf/parent/executable fixture with rebased data pointers and chained
   imports writes the exact four-line native order above. Each callback checks
   its dependency's mutation; main checks all mutations. Add two callbacks in
   one image to check array order. Independently compare native Apple output.
2. A diamond with one shared leaf initializes the leaf once. Reverse the supplied
   library input list and require identical output and placement.
3. Repeat the same prepared image three times; each begins with pristine data,
   runs every callback once and produces the same full outcome under interpreter
   and both Linux JIT indirect-dispatch modes.
4. Corrupt array size/alignment, writable offset storage, target into data/gaps,
   out-of-image targets and overflowing offsets. All reject before stdout or
   guest writes. A nonreturning constructor exhausts the shared budget and never
   prints main's marker. A constructor invoking exit prevents main execution.

These are proposed Sable tests. The current native companion deliberately also
uses TLS and Foundation, so it cannot serve as a passing isolated gate-1 fixture.

## Gate 2: plain C Mach-O TLV storage and resolver

Start with static plain C TLS in supplied original images and the existing
single-thread scalar engine. Save a pristine template spanning initialized and
zero-fill sections, including checked layout/alignment. Process descriptor arrays
before any constructor can use TLS. Resolve a descriptor to one private allocation
per image and logical thread, plus its validated offset; repeated lookups return
the same address. A different logical context and every fresh run get fresh
copied/zeroed values. Constructor writes must never alter the saved template.
Use bounded guest storage allocated for that run and release it on teardown.
Full pthread creation/join is a later execution requirement; two host-driven
logical contexts can test the storage owner without pretending pthreads work.

On-disk descriptors have a resolver pointer, key and offset, three 64-bit words.
The resolver takes the descriptor address in x0 and returns its variable address
in x0. It is a special calling convention: preserve x1–x15, x18–x29 and all Q
registers; x16/x17 are scratch and LR is the call's return link. LLVM states the
[Darwin TLS preserved set](https://github.com/llvm/llvm-project/blob/main/llvm/lib/Target/AArch64/AArch64CallingConvention.td#L781).
A scalar-only first step can preserve all modeled scalar state through a trusted
resolver bridge and continue rejecting unsupported vector instructions; it must
not claim the full vector ABI. No host function pointer may enter guest memory.

Apple's current runtime rewrites these disk records into a packed v2 layout.
Do not confuse that private runtime form with three untouched disk words, or
execute Apple's bootstrap as a working allocator. A first-party resolver can
keep its own checked lookup data; observable variable addresses, values and call
preservation are the contract. Sources:
[descriptor definitions](https://github.com/apple-oss-distributions/dyld/blob/fd8d0c4d52320ebf64db34f3cb280310d905c5ae/libdyld/ThreadLocalVariables.h#L109),
[template setup and allocation](https://github.com/apple-oss-distributions/dyld/blob/fd8d0c4d52320ebf64db34f3cb280310d905c5ae/libdyld/ThreadLocalVariables.cpp#L62),
[ARM64 resolver](https://github.com/apple-oss-distributions/dyld/blob/fd8d0c4d52320ebf64db34f3cb280310d905c5ae/libdyld/threadLocalHelpers.s#L203).

Bounded regressions:

1. Use original C initialized=17/zero=0 variables. Constructor changes main's value
   to 23; repeated resolver calls preserve address/value. A second logical context
   still sees 17/0, writes 41/9 and leaves main at 23/7. This matches the observed
   native companion. The host-context test is not a native pthread pass.
2. Two images with equal descriptor offsets must have independent storage. Repeat
   a prepared process three times and require fresh templates and full outcome
   equality across interpreter/JIT. Include zero-only templates and alignment
   padding. Add vector-preservation sentinels once vector state is implemented.
3. An original assembly caller checks every modeled preserved scalar register,
   return address and SP on first and repeated lookup. Reject descriptors outside
   owned arrays, non-24-byte array sizes, overflowing/out-of-template offsets,
   overlapping templates and allocation-cap exhaustion without host access.
4. Keep C++ TLS constructors/destructors, TLS initializer-pointer sections,
   `_tlv_atexit`, shared-cache static keys and dynamic TLS loading unsupported.
   S3's multiple-thread teardown gate stays open until actual guest pthread
   lifecycle and cleanup pass the companion end to end.

## Remaining evidence limits

The explicit VM observations are a useful later regression oracle; no privileged
tracing, root/DTrace, Mach IPC or kernel-entry identification was performed.
No Foundation VM/TLS internal call path was established. These two gates do not
resolve the current missing Foundation/UIKit dependencies or provide the ObjC
runtime. They are prerequisites justified by original startup behavior, not a
promise that implementing them alone will run Notes.

The three real Notes UI/Metal checks in `evidence/app-readiness/RESULTS.md` remain
native reference results. This research ran no Sable UI interaction, rendered no
Sable frame, allocated no AWS resource and tested no graphics backend. Native
Simulator uses the Mac's graphics services. GPU-free compatibility, software
rendering performance and comparative workflow cost remain unproved; retain
`docs/app-benchmarks.md` as the app/GPU gate.

Read each requested CI run once, with no polling:
[42849999081](https://github.com/kashyab12/keld/actions/runs/42849999081) at
`f5af75c` and [42917220032](https://github.com/kashyab12/keld/actions/runs/42917220032)
at `5170468` both completed successfully on ubuntu-24.04 and macos-15. No pending
job remained in those reads. This research did not rerun the full Rust or cloud
checks; only the native probes, metadata inspection and existing Sable startup
attempts described here were executed.
