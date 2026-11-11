#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
bash scripts/build-instances.sh
cargo build --quiet --release --locked
engines=(interp)
if [[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]]; then engines+=(jit); fi
for format in absolute compact; do
 for mode in chained legacy; do
  for split in 0 1; do
   out="artifacts/instances/$format/$mode/$split"
   libs=(--lib "$out/libsable_objc.dylib" --lib "$out/libsable_system.dylib")
   if [[ "$split" == 1 ]]; then
    libs+=(--lib "$out/libsable_base.dylib")
    python3 scripts/check-instance-format.py "$format" "$out" --split > "$out/format-records.json"
   else
    python3 scripts/check-instance-format.py "$format" "$out" > "$out/format-records.json"
   fi
   for engine in "${engines[@]}"; do
    for ctor in 0 1; do
     target/release/sable run "$out/instance-$ctor.macho" "$engine" "${libs[@]}" > "$out/$engine-$ctor.txt"
     printf 'Guest instances, pointer methods, zeroing, isolation and disposal OK\n' | cmp - "$out/$engine-$ctor.txt"
    done
   done
  done
 done
done
echo 'Fixed-layout instance creation/disposal, object-pointer methods, zeroing, isolation, ABI and constructor state passed.'
