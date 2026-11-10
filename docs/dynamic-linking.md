# Restricted eager dynamic linking

Sable implements a first-party Mach-O linking path rather than executing Apple's
dyld. `Image::link(main, libraries)` receives the exact object bytes the caller
supplies. The CLI accepts repeated `--lib PATH` arguments. Dependencies match
`LC_ID_DYLIB` identities exactly; Sable never interprets install names as permission
to search the host filesystem or download a library.

The main executable remains at its preferred address. Reachable libraries are
placed deterministically starting at guest address `0x200000000`, with 256 MiB
spacing. The loader preserves original segment ordinals including inaccessible
`__PAGEZERO`, slides addresses, applies supported pointer rebases, resolves export
tries, then applies regular and lazy pointer binding eagerly. Positive library
ordinals resolve against the importing object's declared dependency order;
ordinal 0 selects that object and -1 selects the main executable. Input file order
does not change dependency resolution or placement. Required compatibility
versions are checked against supplied libraries' current versions.

Regular and absolute exports are supported. Re-exports, weak definitions/imports,
flat lookup, threaded bind opcodes and resolver exports fail
explicitly. Bounded C constructors now use a checked dependency plan and shared
run budget; see [image initialization](initializers.md). Terminators and TLS
sections remain rejected. Lazy streams
are resolved before execution; Sable does not implement Apple's lazy binder or
general dlopen/dlsym behavior. The original fixtures deliberately use non-lazy
function-pointer imports and need no lazy-binding helper.

Unauthenticated 64-bit chained fixups and separate export tries are now supported
by the same explicit-library path. Supported formats, validation limits and
native evidence are documented in [chained fixups](chained-fixups.md). This does
not add shared-cache or framework startup support.

ULEB/SLEB readers check bounds and overflow. Export tries limit depth, visits,
name lengths and cycles. Pointer fixup slots must lie fully within non-executable
storage. Region addresses and pointer arithmetic are checked; loaded image memory
is capped at 64 MiB, code at 1 MiB, individual dyld streams collectively at 8 MiB,
and fixup operations are bounded. Read-only-after-fixup segments become immutable
before guest execution. Failed links discard their private work and return no image.

Code sections from all images become distinct executable ranges. Fetch and native
indirect dispatch validate those ranges, alignment and instruction budgets. A gap
between ranges cannot become an implicit fallthrough into another image. Native
dispatch tables contain signed offsets to trusted generated entry points, not
guest-controlled host addresses. All exits restore the host's saved registers.

`Segment` bytes use `Arc<Vec<u8>>`. Fixup construction uses copy-on-write before
the image is exposed. Every run clones writable bytes into unique ownership and
creates a fresh stack. Immutable image bytes share their existing allocations.
The native buffer-pointer table is valid for the entire exclusive run; private
buffers cannot resize during execution. Native stores are generated only for
regions with write permission. Prepared execution borrows its immutable image
and reuses translated code, never a previous run's writable state.

The minimal original `libsable_system.dylib` declares the standard LibSystem
identity so the original executable's imports can bind to a documented Darwin
ABI subset. It implements only `_write`, `_exit` and `_getpid` through SVC. It is
not Apple's LibSystem, contains no Apple implementation, and supplies no pthread,
TLS, malloc, errno storage, Objective-C or initialization support. On the native
Apple oracle, the same executable resolves that identity to Apple's real
installed LibSystem; Sable resolves it to the explicitly supplied original library.

Format references: [Apple Mach-O definitions](https://github.com/apple-oss-distributions/xnu/blob/main/EXTERNAL_HEADERS/mach-o/loader.h),
[LLVM's Mach-O writer](https://github.com/llvm/llvm-project/blob/main/lld/MachO/SyntheticSections.cpp).
This code is an original implementation of the restricted format contracts.
