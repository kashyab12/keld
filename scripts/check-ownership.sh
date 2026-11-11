#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
bash scripts/build-ownership.sh
cargo build --quiet --release --locked
sable="${CARGO_TARGET_DIR:-target}/release/sable"
engines=(interp)
if [[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]]; then engines+=(jit); fi
for format in absolute compact; do
 for mode in chained legacy; do
  out="artifacts/ownership/$format/$mode"
  # Independent metadata inspector checks actual methods, fixup commands and CF records.
  python3 scripts/check-ownership-format.py "$format" "$out" > "$out/format-records.json"
  for engine in "${engines[@]}"; do
   for client in client observer; do
    "$sable" run "$out/$client.macho" "$engine" --main --lib "$out/CoreFoundation.dylib" --lib "$out/Foundation.dylib" --lib "$out/libsable_system.dylib" --lib "$out/libsable_objc.dylib" > "$out/$engine-$client.txt"
    if [[ "$client" == client ]]; then
     printf 'SABLE FOUNDATION\n' | cmp - "$out/$engine-$client.txt"
    else
     printf 'Ownership, pools, guest dealloc, x20, ctor/TLV OK\n' | cmp - "$out/$engine-$client.txt"
    fi
   done
  done
 done
done
echo 'Original ARC ASCII uppercase, ordinary ownership, guest dealloc and main/ctor/TLV checks passed.'
