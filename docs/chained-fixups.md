# Chained pointer fixups

Sable now links original Mach-O images using `LC_DYLD_CHAINED_FIXUPS` and
`LC_DYLD_EXPORTS_TRIE`. This extends the eager explicit-library linker; it does
not execute Apple's dyld or discover frameworks on the host.

The decoder supports unauthenticated pointer formats `DYLD_CHAINED_PTR_64` (2)
and `DYLD_CHAINED_PTR_64_OFFSET` (6), import formats 1/2/3, positive library
ordinals, self/main ordinals, inline unsigned pointer addends and signed table
addends. It preserves the pointer's high byte and distinguishes absolute from
image-relative rebase targets. The binary layout reference is Apple's
[pinned fixup-chains.h](https://github.com/apple-oss-distributions/dyld/blob/fd8d0c4d52320ebf64db34f3cb280310d905c5ae/include/mach-o/fixup-chains.h).
The Rust implementation is original; no Apple implementation or header is
vendored or linked into this MIT module.

Metadata validation checks version, symbol compression, segment/table bounds,
overlapping records, format/reserved bits, import ordinal/name bounds, page size,
page count and pointer strides. A pointer must occupy eight file-backed bytes
inside its page and a non-executable mapped segment. Segment offsets must agree
with the image's actual mapping. Operation counts remain bounded. The decoder
resolves every import and builds a patch list before changing any bytes.
Read-only-after-fixup permissions apply after patching.

ARM64e/PAC, weak imports, compressed symbols, multiple chains per page and mixed
legacy/chained fixup streams remain explicit errors. Other pointer formats and
general shared-cache linking are unsupported. Data-only dylibs may export data
without a nonempty code section; they contribute no executable range. An empty
main executable is still rejected.

## Evidence

`SABLE_FIXUPS=chained bash scripts/build-dynamic.sh` builds original code with
LLVM's chain encoder. `bash scripts/check-chained.sh` rebuilds and executes four
programs under the interpreter and, on x86-64 Linux, the native translator.
Neither requires an Apple SDK or runtime.

Three C workloads exercise transitive function imports and rebased private data.
An additional assembly workload imports two addresses with offsets +8 and -16,
uses an image-local rebased pointer and emits 51. Its data-only numbers dylib
caught an earlier assumption that every dylib must contain executable code.
Apple's installed Simulator independently executes these same programs and
checksums. LLVM emits pointer format 2 and import formats 1/2 for these fixtures.

Format 6, tagged high bytes and format-3 64-bit import addends have synthetic
format tests; they do not yet have independent native fixture evidence. Malformed
metadata/slots and 2,000 deterministic metadata mutations exercise decoder
rejection and bounds. Repeated runs check private state/reset; Linux tests compare
full interpreter/native outcomes with both indirect dispatch modes.

The original UIKit app now reaches a missing UIKit dependency instead of the
previous unsupported-command rejection. This is a startup diagnostic, **not an
app pass**: dependency resolution still stops before its chained pointers are
applied or its code executes. No UIKit implementation, Objective-C runtime,
rendering backend or input service is supplied. See the
[execution receipts](../evidence/chained/RESULTS.md).
