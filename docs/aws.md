# AWS Linux execution check

```sh
# Uses your configured AWS CLI identity and default VPC in us-east-1.
python3 scripts/aws-smoke.py run
# Dynamic libraries, prepared execution, dispatch controls and journals:
python3 scripts/aws-smoke.py run --dynamic --output evidence/s2/new-run
# Separate nested-KVM experiment on a small C8i host:
python3 scripts/aws-smoke.py run --firecracker --output evidence/sandbox/new-run
```

The default CPU check launches one Ubuntu 24.04 x86-64 `c7i.large` with a 12 GiB gp3 root
disk, delete-on-termination, IMDSv2, an owned SSH key and caller-IP-only port 22.
No KVM, nested virtualization, GPU, macOS guest, Apple runtime or remote Mac is
used. Rust 1.93.1 and Clang are installed, then our source and original fixtures
are transferred over SSH.

`--firecracker` is a separate experiment on `c8i.large` with nested virtualization
explicitly enabled. It verifies KVM, builds static Sable, and runs the pinned
Firecracker/jailer harness on that owned host. It uses 31 randomized blocks of
an unisolated process control and fresh 128/256 MiB Linux microVMs. Only original
CPU/runtime fixtures enter the guest. Logs, sample journals, configuration hashes,
namespace/privilege records and resource measurements are under
`results/firecracker`. The harness requires root on that disposable host; never
run it on a shared developer machine. Routine checks only check its syntax.
See [sandbox design and measurement limits](sandbox-design.md).

Every new run records PCI graphics devices, exposed GPU nodes, optional Vulkan
and NVIDIA tool results, and renderer overrides in
`results/graphics-inventory.json`. Missing graphics tools do not prove the
absence of a GPU. This is a CPU execution check: no UIKit app, input automation,
Metal renderer or software graphics fallback runs here. The native app oracle
and the required future AWS app tests are described in
[app and graphics gates](app-benchmarks.md).

The remote run checks formatting, Clippy, the release test suite, recompiles the
Mach-O fixtures with LLVM, compares rebuilt binaries, runs hello and compute
through native x86 translation, and executes nine alternating benchmark pairs.
It also rebuilds and checks scalar TLS and static scalar Objective-C2 fixtures,
including constructor access, guest method bodies, ABI checks and explicit
unknown-message rejection. The protocol matrix checks names, exact identities,
declared adoption, real method execution, constructor access and mutable storage
with both method-list and fixup formats. These are console programs, not UIKit
apps. The superclass matrix additionally checks same-image/dylib parents, inherited and
Super2 method calls, ABI, direct-adoption controls and explicit depth rejection.
The instance matrix checks real creation/disposal, pointer methods, requested
extra-byte zeroing/isolation, ABI and constructor/main/TLV state in both engines.
The payload matrix adds four root/child sizes through 64 bytes, guest Q getters
and void setters, super sums, exact extra-byte boundaries and fresh-run state.
Its 64 compiled variants execute in both engines; independent format reports
inspect emitted ivar cells, layouts and signatures.
The `--extended` path runs three independent oracle seeds in interpreter/JIT modes.
It also rebuilds and checks four original chained-fixup programs in both engines;
the eight outputs and rebuilt binary hashes are retained under `results/chained`.
Logs are saved under `evidence/aws-smoke/`. The internal translator/reference ratio
is only an arithmetic dispatch comparison. It is not an iOS UI or Macbox result.
The observed runs and their limitations are recorded in [the evidence report](../evidence/README.md).

An automatic guest shutdown is scheduled after 45 minutes with instance shutdown
configured to terminate. The controller also tears down in `finally`, waits for
termination, removes its security group and EC2 public key, and queries for zero
remaining active instances, tagged volumes, security groups and keys. Bootstrap
and remote execution have separate 12-minute limits. The guest shutdown timer is
an extra guard that depends on cloud-init reaching that line, not a cloud billing
budget or a guarantee against control-plane/network failures.

Private recovery state and the local SSH private key are written to this Thread's
data directory (`MAKO_THREAD_DATA_DIR`) or `~/.cache/keld-sable`, outside Git. If the
controller is interrupted, use the printed state path:

```sh
python3 scripts/aws-smoke.py cleanup /path/to/private/state.json
```

Cleanup finds a possibly ambiguous launch by its unique EC2 client token; it does
not replay an unknown launch. It only targets its own run. If cleanup fails,
inspect the private state and finish cleanup before starting another run.

AWS CLI's live pricing response on 2026-11-10 quoted **$0.08925/hour** for this
Linux On-Demand instance in us-east-1. That is a rate, not an observed invoice;
disk, IPv4, transfer and taxes are extra. Confirm your own rate before rerunning.

## Gated concurrency experiments

`--firecracker --concurrency 1 4 16 32 64 96 150` selects one owned
`c8i.2xlarge` (8 vCPUs / 16 GiB), not 150 EC2 instances. Each microVM receives a
distinct network namespace, UID, jail and cgroup. Guests wait on serial input
until the controller observes every requested guest simultaneously ready.
They remain resident for three seconds, execute checked original Sable CPU
workloads together, wait for a second release, and exit. No guest network or
host filesystem share is added. This tests CPU session density, not iPhone UI
capacity. A guest failure rejects the wave and preserves its logs.

```sh
python3 scripts/aws-smoke.py run --firecracker --concurrency 1 16 64 150 \
  --work-repetitions 20 --memory-sweep --pin-cpus \
  --output evidence/density-new
```

The optional memory sweep runs 128 MiB then 64 MiB profiles on the same host;
the first failure stops subsequent profiles. CPU pinning is an experimental
round-robin single-host-CPU assignment per VMM. It is not a documented fix for
all nested-KVM failures. `--work-repetitions` repeats the fully checked CPU
workload, not a UI loop. Peak guest charge, aggregate VMM PSS, host available
memory, CPU counters, timing events and cleanup receipts are retained. The
[density report](../evidence/density/RESULTS.md) distinguishes passing waves
from failed launches and the native [video reference](../evidence/recordings/README.md).
Run `python3 scripts/density-report.py` to re-audit saved passing waves and
generate the timestamped event replay. It creates no cloud resources.
