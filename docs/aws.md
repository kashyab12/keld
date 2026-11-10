# AWS Linux execution check

```sh
# Uses your configured AWS CLI identity and default VPC in us-east-1.
python3 scripts/aws-smoke.py run
# Dynamic libraries, prepared execution, dispatch controls and journals:
python3 scripts/aws-smoke.py run --dynamic --output evidence/s2/new-run
```

The script launches one Ubuntu 24.04 x86-64 `c7i.large` with a 12 GiB gp3 root
disk, delete-on-termination, IMDSv2, an owned SSH key and caller-IP-only port 22.
No KVM, nested virtualization, GPU, macOS guest, Apple runtime or remote Mac is
used. Rust 1.93.1 and Clang are installed, then our source and original fixtures
are transferred over SSH.

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
