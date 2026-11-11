#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
bash scripts/build-payload.sh
cargo build --quiet --release --locked
engines=(interp)
if [[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]]; then engines+=(jit); fi
for format in absolute compact; do
 for mode in chained legacy; do
  for profile in 0 1 2 3; do
   for split in 0 1; do
   out="artifacts/payload/$format/$mode/$profile/$split"
   libs=(--lib "$out/libsable_objc.dylib" --lib "$out/libsable_system.dylib")
   if [[ "$split" == 1 ]]; then
    libs+=(--lib "$out/libsable_base.dylib")
    python3 scripts/check-payload-format.py "$format" "$out" "$profile" --split > "$out/format-records.json"
   else
    python3 scripts/check-payload-format.py "$format" "$out" "$profile" > "$out/format-records.json"
   fi
   for engine in "${engines[@]}"; do
    for ctor in 0 1; do
     target/release/sable run "$out/payload-$ctor.macho" "$engine" "${libs[@]}" > "$out/$engine-$ctor.txt"
     printf 'Payload fields, void setters, super sums, extra bounds and reset OK\n' | cmp - "$out/$engine-$ctor.txt"
    done
   done
  done
 done
done
done
echo 'Q payload fields, void setter effects, inherited/super access, size/extra boundaries and private reset passed.'
