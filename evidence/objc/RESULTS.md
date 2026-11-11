# Scalar Objective-C2 execution

Sable now registers original root-class metadata and executes compiled ARM64
instance/class methods through an original scalar messenger. These are console
programs, not UIKit apps or iPhone simulators. See the [implementation contract](../../docs/objc.md).

## What actually executed

| Path | Program and result |
| --- | --- |
| Native Apple reference on ARM64 Mac | Four final original ObjC fixtures (plain/constructor, legacy/chained) passed on iOS 27.2 Simulator build 24B8117, plus four prior control programs. This uses Apple's installed local libobjc, not Sable. |
| Sable interpreter on ARM64 Mac | The same four original Mach-O files passed. Unknown nonnil messages explicitly failed. All 47 final local Rust tests passed. |
| Sable on x86 EC2, attempt 01 | All 52 Rust tests passed, including the checked-in ObjC fixtures in interpreter and both JIT dispatch modes. The subsequent Linux fixture rebuild failed because the older Clang did not accept `-fno-objc-relative-method-lists`; this is a failed full run. |
| Sable on x86 EC2, attempt 02 | All 52 Rust tests passed. The compiler capability fallback then failed because the script depended on unavailable `rg`; this is another failed full run. |
| Sable on x86 EC2, attempt 03 | Full run passed before the final C-only string-cap regression fix: 52 Rust tests, all eight rebuilt ObjC interpreter/JIT CLI passes, explicit unknown-message rejection, and 47,610 independent full-state oracle cases. |
| Sable on x86 EC2, attempt 04 | Final source passed: 53 Rust tests, all eight rebuilt ObjC interpreter/JIT CLI passes, explicit unknown-message rejection, and 47,610 independent full-state oracle cases across three seeds. Runtime source hashes match this checkout. |
| Actual Notes UIKit executable in local Sable | Exit 125, `unsupported ObjC section: __objc_methlist`, empty stdout, zero interactions. The earlier checkpoint's missing-UIKit diagnostic is now preceded by this explicit unsupported metadata check. |

The native reference creates a private Simulator set, boots only its owned
device, runs the original binaries and deletes it. The final
[native receipt](native-reference.json) records eight successful programs,
delete exit 0 and zero remaining devices. No Apple framework, SDK, runtime,
UIKit binary, video or xcresult was transferred to EC2 or committed. The
checked-in binaries contain original source and LLVM linker metadata only.

## Behavior checked

Two original classes in separate images exercise actual compiled instance/class
IMP instructions, a nested message, nil scalar return 0, absent class lookup 0,
and explicit failure for an unknown nonnil selector. Both images' `classValue:`
references must produce the same SEL. Each method checks its receiver/selector
and argument values. The constructor variant sends messages before main and
checks pristine state before initialization. Repeated prepared runs compare full
outcomes and require fresh object/static storage.

An original assembly caller and IMP capture self, _cmd, ordered x2–x7 values,
x8, the real incoming x18/frame pointer, SP and actual return link. The compiled
ABI method is a direct guest tail branch to that capture, visible in the
[original disassembly](original-disassembly.txt). x16/x17 are scratch and
x9–x15 are caller-clobbered; the native ABI proof makes no preservation claim
for those channels. The tail messenger does not reset CPU state or synthesize
a method's return value.

Seven new Rust test functions cover successful execution and negative cases:
malformed root/metaclass graphs, RO flags/size, ivars, type encodings, duplicate
methods, relative lists, foreign IMPs, section/file bounds, missing dependencies,
mutated prepared/live metadata, foreign private traps, invalid receivers/selectors
and every budget edge in a lookup/tail-branch/IMP-return sequence. A loader
regression also ensures the ObjC metadata cap does not narrow ordinary C
string-section support. Linux tests
compare full interpreter/JIT CPU, remaining budget, flags, memory, output and
exit state in both indirect-dispatch modes. The tests execute compiled methods;
metadata acceptance alone is not counted as execution.

CSEL/CSINC/CSINV/CSNEG were added because the optimized source emits conditional
selects. The local Unicorn 2.1.4 interpreter oracle passed 7,935 full-state cases,
including 2,048 new condition/width/invert/increment/alias cases. The corpus also
retains logical masks, conditional compare, memory, call and branch coverage.
The [local oracle receipt](oracle-interp.json) contains its input hash.

The oracle runner now builds its input in a growing byte buffer instead of
repeatedly copying an immutable prefix. A [one-pair local comparison](oracle-input-buffer.json)
produced identical 39,055,736-byte inputs: 12.749 seconds for repeated copying
versus 0.032 seconds for the mutable append loop, mutable measured first.
This is test preparation only, not a Sable execution or app speedup. New oracle
receipts separately record `input_build_elapsed_seconds` (corpus generation plus
input assembly); `elapsed_seconds` covers batch execution and independent checks.
No matched-host total-cloud speedup is inferred from the two cloud runs.

## Reproduction and provenance

```sh
bash scripts/check-objc.sh
cargo test --release --test objc
python3 scripts/apple-oracle.py --objc --output artifacts/objc-native.json
python3 scripts/aws-smoke.py run --extended --output artifacts/objc-aws
```

The build uses headerless freestanding Clang plus Rust's LLVM Mach-O linker.
The linker always requests absolute method lists. A frontend capability probe
uses the explicit switch where available; an older compiler without that switch
must still produce final records accepted by the checked loader. The first
failed EC2 attempt is retained rather than relabeled successful.

[Source/fixture hashes](fixture-sha256.json) cover the final native-checked
binaries and original fixture sources. [Attempt 01](aws-01/run.json) retained its
logs and [zero-resource cleanup receipt](aws-01/cleanup.json). Its snapshotted
runtime matches the tested code; the module README was corrected afterward.
Attempt 02 also retained its [logs](aws-02/stderr.txt) and
[zero-resource cleanup receipt](aws-02/cleanup.json). Both failures were in the
build scripts; no core runtime change was needed. The corrected older-compiler
branch was then [exercised locally](old-frontend-branch.json) with an explicit
capability-rejection wrapper before retrying the actual older cloud compiler.

The [final EC2 run](aws-04/run.json) used Ubuntu 24.04 on an x86-64
`c7i.large`, two vCPUs, Rust 1.93.1 and the installed Clang 18 frontend. Its
[raw stdout](aws-04/stdout.txt), [stderr](aws-04/stderr.txt),
[source hashes](aws-04/runtime-source-sha256.json), [runner hashes](runner-sha256.json)
and individual oracle/fixture outputs are retained. The 47,610-case figure is
7,935 cases × three seeds × interpreter/JIT. Both JIT dispatch modes are also
covered by the Rust execution/negative/reset tests; the CLI and Unicorn runs use
the default JIT mode. All oracle input hashes match attempt 03 after the runner
buffer change. [Attempt 03](aws-03/run.json) and the final run both completed
successfully; neither is a UIKit or sandbox-density experiment. All four hosts
have verified zero remaining instances, volumes, security groups and keys;
[attempt 03 cleanup](aws-03/cleanup.json), [final cleanup](aws-04/cleanup.json)
and the [verification summary](verification.json) retain those checks.

Mako recipe 12 quick/full checks passed; the corrected fixture builder passed
the focused shell/execution checks again. The recipe is published. The full
suite rebuilds dynamic, chained, initializer, TLS and ObjC fixtures and runs the
expanded independent oracle. CI has the new ObjC rebuild/execute step.

## What this does not prove

No Foundation/UIKit implementation, graphical app interaction, software rendering,
GPU-free app execution, app-scale concurrency or 2–4× advantage over Limrun/Macbox
is established. This EC2 experiment is process-level runtime validation, not a
new Firecracker containment/density run. Earlier 150-guest CPU microVM evidence
remains a separate workload and does not become 150 iPhone simulators.

This profile excludes inheritance/super, categories/protocols, +load/+initialize,
ARC/retain-release/allocation, forwarding, dynamic class changes, caches,
relative/PAC method formats, tagged/nonpointer isa, exceptions, vector/FP/aggregate
returns and guest pthreads. Dispatch checks bounded metadata snapshots at each class/IMP
lookup trap and uses uncached lookup; it is not a measured production fast path.
The extended AWS driver also runs the pre-existing CPU/QEMU microbenchmark;
those timings are not evidence about ObjC apps or competitor performance.
