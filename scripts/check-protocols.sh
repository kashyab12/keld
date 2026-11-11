#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
bash scripts/build-protocols.sh
cargo build --quiet --release --locked
engines=(interp)
if [[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]]; then engines+=(jit); fi
for format in absolute compact; do
 for mode in chained legacy; do
  out="artifacts/protocols/$format/$mode"
  python3 scripts/check-protocol-format.py "$out" > "$out/format-records.json"
  for engine in "${engines[@]}"; do
   for ctor in 0 1; do
    target/release/sable run "$out/protocol-$ctor.macho" "$engine" --lib "$out/libsable_objc.dylib" --lib "$out/libsable_system.dylib" > "$out/$engine-$ctor.txt"
    printf 'Protocol lookup, names, declared adoption and guest IMP OK\n' | cmp - "$out/$engine-$ctor.txt"
   done
  done
 done
done
echo 'Static protocol identity/name/adoption, real IMP, scalar ABI, constructor and mutable storage checks passed.'
