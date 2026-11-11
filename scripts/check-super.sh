#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
bash scripts/build-super.sh
cargo build --quiet --release --locked
engines=(interp)
if [[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]]; then engines+=(jit); fi
for format in absolute compact; do
 for mode in chained legacy; do
  for split in 0 1; do
   out="artifacts/super/$format/$mode/$split"
   libs=(--lib "$out/libsable_objc.dylib" --lib "$out/libsable_system.dylib")
   if [[ "$split" == 1 ]]; then
    libs+=(--lib "$out/libsable_base.dylib")
    python3 scripts/check-super-format.py "$format" "$out" --split > "$out/format-records.json"
   else
    python3 scripts/check-super-format.py "$format" "$out" > "$out/format-records.json"
   fi
   for engine in "${engines[@]}"; do
    for ctor in 0 1; do
     target/release/sable run "$out/super-$ctor.macho" "$engine" "${libs[@]}" > "$out/$engine-$ctor.txt"
     printf 'Superclass lookup, Super2 guest methods, nesting and ABI OK\n' | cmp - "$out/$engine-$ctor.txt"
    done
    set +e
    target/release/sable run "$out/super-2.macho" "$engine" "${libs[@]}" > "$out/$engine-deep.txt" 2> "$out/$engine-deep.stderr"
    code=$?
    set -e
    [[ "$code" == 125 && ! -s "$out/$engine-deep.txt" ]]
    [[ "$(< "$out/$engine-deep.stderr")" == *'ObjC hierarchy exceeds depth two or cycles'* ]]
    if [[ "$split" == 0 ]]; then
     target/release/sable run "$out/super-3.macho" "$engine" "${libs[@]}" > "$out/$engine-conformance.txt"
     printf 'Superclass lookup, Super2 guest methods, nesting and ABI OK\n' | cmp - "$out/$engine-conformance.txt"
    fi
   done
  done
 done
done
echo 'Same-image/dylib superclass lookup, scalar Super2, ABI, nesting, constructor/reset and depth rejection passed.'
