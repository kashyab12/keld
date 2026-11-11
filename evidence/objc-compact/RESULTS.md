# Compact Objective-C method-list execution

Sable now executes the same original scalar root-class programs with either
24-byte absolute or 12-byte on-disk relative method lists. This addresses the
previous `__objc_methlist` section rejection. It is a representation gate;
Foundation/UIKit, graphical interaction and a competitive app speedup remain
unproved. [Format and runtime contract](../../docs/objc.md).

## Execution paths

| Path | Observed result |
| --- | --- |
| Native Apple reference on ARM64 Mac | All eight final ObjC programs passed: absolute/compact × legacy/chained × plain/constructor. Four prior controls also passed. iOS 27.2 Simulator build 24B8117; this runs Apple's local runtime, not Sable. |
| Sable interpreter on ARM64 Mac | The same eight original programs passed, with four explicit unknown-message rejections across the two formats/fixup modes. Mixed-format images, private reset, ABI and metadata-negative checks also passed. |
| Local project checks | Recipe 13 quick/full checks passed and were published. The Rust suite has 50 tests on this ARM64 Mac. |
| Sable on x86 EC2, attempt 01 | Failed test run. Both guards rejected unsafe metadata operations, but two new assertions incorrectly required identical backend diagnostic text. Raw logs and cleanup are retained. |
| Sable on x86 EC2, attempt 02 | Final source passed 56 Rust tests, sixteen rebuilt absolute/compact CLI executions, eight explicit unknown-message rejections and 47,610 independent full-state CPU oracle cases. |
| Actual Notes UIKit executable, local Sable | Exit 125 at `unsupported ObjC section: __objc_protolist`, empty stdout, zero interactions. This is the next loader rejection, not execution of the app. |

The [native receipt](native-reference.json) records twelve program passes and
owned-device cleanup: delete 0, zero remaining devices. No Apple SDK, framework,
runtime, UIKit binary, video or xcresult entered EC2 or this commit. The new
Mach-O files contain original headerless source and LLVM metadata only.

## What changed and what was checked

The loader accepts exactly header `0x8000000c` in owning `__objc_methlist` storage.
The three i32 displacements are each relative to their own field's post-slide
address. Real negative IMP displacements are exercised. A selector target must
be a complete eight-byte slot in the owning selref section; types and code
must belong to the owning image. Existing root-class and scalar-signature
restrictions still apply. Shared-cache direct selectors, signed/PAC forms and
unknown flags/strides remain rejected.

Canonicalization changes selref pointers and leaves relative entries untouched.
The fixture matrix checks exact table-byte equality before/after linking and
execution, four-byte-only list alignment, exclusion from guest instruction ranges,
class/instance/nested calls, constructor access, nil, unknown class/message,
cross-image SEL equality, assembly caller/IMP ABI, reordered library input and
three fresh prepared runs. Mixed absolute/compact images are also checked.
Linux tests compare complete outcomes in both JIT indirect-dispatch modes.
Guard-fault probes compare rejection and complete CPU/budget/memory state;
native guard diagnostic strings are intentionally different from the interpreter.

Negative checks cover field-relative arithmetic underflow/overflow; bad count,
stride/flag bits, partial headers/entries, slot interior/foreign pointers,
invalid types/IMPs, unaligned list pointers, executable-section flags, malformed
compact records alongside valid absolute metadata, prepared/live selector
mutation, attempted stores to compact RX bytes, and branches into those bytes.
Budget boundaries now exercise both formats. The C-only large-string regression
remains in the suite.

A test initially assumed that an incorrectly based IMP displacement must always
be rejected. That is false when it resolves to another aligned address inside
owning code. The corrected assertion verifies the actual field-relative target;
behavioral checks still require the intended method's result. The loader does
not infer source-level function boundaries or replace an IMP with a synthetic
successful result.

The independent [format inspector](../../scripts/check-objc-format.py) runs after
each original LLVM build. It fails if the requested compact format was not
emitted, validates section/header/field targets and records hashes, including
negative IMP offsets. Paired [chained](chained-format-records.json) and
[legacy](legacy-format-records.json) records are retained. Frontend flags alone
do not establish the final format; the linker explicitly selects each profile.

## Reproduction and provenance

```sh
bash scripts/check-objc.sh
cargo test --release --lib objc::tests
cargo test --release --test objc
python3 scripts/apple-oracle.py --objc --output artifacts/objc-native.json
python3 scripts/aws-smoke.py run --extended --output artifacts/objc-compact-aws
```

[Source/fixture hashes](source-fixture-sha256.json) cover the original source,
checked-in new binaries and runners. The build always retains the absolute
control and additionally emits the compact matrix. Existing absolute fixtures
are unchanged. The [final EC2 run](aws-02/run.json) used Ubuntu 24.04 on an
x86-64 `c7i.large`, two vCPUs, Rust 1.93.1 and Clang 18. [Raw stdout](aws-02/stdout.txt),
[stderr](aws-02/stderr.txt), [runtime hashes](aws-02/runtime-source-sha256.json)
and both emitted format profiles are retained. Runtime hashes match the final
checkout. The first [failed run](aws-01/run.json) is retained; only the test
source changed before retrying. Its two guard probes now compare fault state
rather than diagnostic strings. The CPU oracle is 7,935 cases × three seeds ×
interpreter/JIT; both JIT dispatch modes are additionally covered by Rust tests.
This is process-level validation, not a new microVM test. The pre-existing CPU/QEMU
microbenchmark also ran 186 samples with zero failures; those numbers do not
establish ObjC app or competitor performance. CI now rebuilds, inspects and executes both formats; its public
result should be read at the final commit, not inferred from local checks.

The retained [actual app attempt](app-still-unsupported.json) has zero Sable UI
interactions. Earlier native app video remains an Apple-only reference, and
previous 150-guest CPU microVM results remain a separate workload. This change
adds no allocation, inheritance/super, protocols/categories, ARC, +load/+initialize,
forwarding, framework supply, rendering, SIMD/FP/aggregate ABI or guest threads.
There is no new Firecracker/density test or Limrun/Macbox app comparison.

Both disposable EC2 attempts have verified zero remaining instances, volumes,
security groups and keys; [failed-attempt cleanup](aws-01/cleanup.json),
[final cleanup](aws-02/cleanup.json) and the [verification summary](verification.json)
retain those checks. No owned native/cloud processes remain.

## Subsequent boundary correction

Independent review found that a deliberately enlarged instruction-section header
could alias compact metadata, making it count as code and admitting a metadata
IMP. The original unmodified positive fixtures remain valid. The
[overlap fix and regression receipts](../objc-boundary/RESULTS.md) reject these
malformed aliases at parse/preparation time, independent of section name/order.
No host escape or successful guest execution from metadata was demonstrated.
