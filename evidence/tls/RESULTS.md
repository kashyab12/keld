# Sable executes scalar Darwin TLV on Linux

Observed November 10, 2026 (Pacific). Sable now runs the original two-image
plain C TLS fixtures on x86-64 EC2, including constructor access, register
preservation and fresh prepared-run state. The same checked-in ARM64 Mach-O
console programs also passed on Apple's native iOS Simulator. **These programs
have no app UI. UIKit remains unsupported.**

| Path | Observed result |
| --- | --- |
| Apple iOS Simulator 27.2 / build 24B8117, ARM64 | Four new TLS programs and four existing controls passed; zero owned devices after cleanup |
| Sable interpreter, local ARM64 Mac | Both fixup formats, constructor/plain variants passed; 40 Rust tests passed |
| Sable on owned EC2 C7i.large, Ubuntu 24.04, x86-64 | 46 Rust tests passed, including interpreter and both JIT dispatch modes; all eight rebuilt TLS CLI outputs matched |
| Independent Unicorn 2.1.4, EC2 | 5,887 full-state cases per seed/engine; three seeds, interpreter and JIT: 35,322 passing cases |
| Actual Notes UIKit executable, local Sable | Exit 125, missing UIKit, zero completed interactions |

The two original dylibs begin with 17/0 and 29/0, then end at 23/7 and 41/9.
The original assembly caller checks x1–x15, x18–x30 and SP immediately across
first/repeated resolution, retaining native x18/frame baselines and comparing
the actual BLR resume link. The constructor variant resolves and modifies A
before main; each fresh prepared run must see the pristine values again. The
Rust regression compares complete CPU/output/private-memory outcomes across
interpreter and JIT modes, including repeated runs and reversed library inputs.

## Implementation exercised

The loader validates TLS regular/zero-fill sections, 24-byte descriptors and
LLVM's separate descriptor-pointer arrays. The linker retains TLS export kind
1 through legacy/chained resolution, requires owning-image descriptor targets
and rejects nonzero descriptor addends. Templates are captured after fixups.

Each process run gets fixed private TLS storage before translation. The
original `__tlv_bootstrap` uses a checked Sable-private trap, returning only a
guest address. Resolution verifies the registered trap/descriptor and its
resolver/key/offset words. It preserves modeled scalar state and uses the
ordinary guest return path and instruction budget. Unsupported forms fail.
See [the contract and caps](../../docs/tls.md).

The compiled ABI fixture also exposed logical-immediate and conditional-compare
instructions absent from the previous decoder. Both were implemented rather
than removing the ABI checks. The expanded oracle covers 1,360 logical-mask
cases and 2,048 conditional-compare cases per seed, in addition to previous
arithmetic, branches and memory checks. Logical immediates translate natively;
conditional compares use the checked Rust fallback inside JIT execution.

Negative tests cover malformed disk sections, missing bootstrap dependencies,
unknown/mid-record descriptors, guest mutation, foreign resolver traps,
invalid prepared offsets/alignment/caps/arena overlap, TLS export/addend
rules and budget edges around BLR/SVC/RET. A zero-only synthetic template and
4 KiB alignment are also exercised. These do not establish guest pthread,
vector state, TLS destructor/C++ initialization or shared-cache/dynamic TLS.

## Receipts and reproduction

The [native receipt](native-reference.json), [fixture/source hashes](fixture-sha256.json),
[local tests](local-tests.txt), [local fixture output](local-fixtures.txt),
[local interpreter oracle](oracle-interp.json), [AWS run](aws-01/run.json),
[AWS stdout](aws-01/stdout.txt), [AWS stderr](aws-01/stderr.txt),
[Linux TLS outputs](aws-01/results/tls) and
[source snapshot](aws-01/runtime-source-sha256.json) identify what ran.
All snapshotted runtime/fixture source hashes were compared with the final
checkout and matched. The Rust tests use the same checked-in binaries executed
by the native reference; the CLI checks additionally rebuild with Linux LLVM.

The [first native attempt](native-initial-failed.json) failed to spawn the four
new files because they lacked executable permissions; the existing controls
passed. File modes were corrected, and a fresh owned simulator run passed all
eight. Both native attempts verified deletion of their owned devices. No
pre-existing simulator device was stopped or modified.

```sh
cargo test --release
bash scripts/check-tls.sh
python3 scripts/apple-oracle.py --tls --output artifacts/tlv-native-new.json
python3 scripts/aws-smoke.py run --extended --output evidence/tlv-aws-new
```

The existing CPU/QEMU regression benchmark also reran under the extended AWS
check. It is not a TLS or app benchmark, and no new speedup claim follows.
The controller verified [zero owned instances, volumes, security groups and
EC2 keys](aws-01/cleanup.json). No Apple runtime, framework, SDK or UIKit app
binary was transferred to Linux. The supplied LibSystem ABI implementation,
probe programs and runtime code are original MIT code.

## Remaining app gate

The actual UIKit executable was attempted again after this change:
[receipt](app-still-unsupported.json), missing
`/System/Library/Frameworks/UIKit.framework/UIKit`, zero interactions. The
native [app video](../recordings/README.md) remains an Apple reference. This
TLV pass is a runtime compatibility step, not Foundation/UIKit execution,
GPU-free rendering, iPhone sandbox capacity or a Limrun/Macbox comparison.
