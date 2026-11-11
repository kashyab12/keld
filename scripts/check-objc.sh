#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
bash scripts/build-objc.sh
cargo build --quiet --release --locked
engines=(interp)
if [[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]]; then engines+=(jit); fi
for engine in "${engines[@]}"; do
  for mode in chained legacy; do
    out="artifacts/objc/$mode"
    for ctor in 0 1; do
      target/release/sable run "$out/objc-$ctor.macho" "$engine" --lib "$out/libsable_other.dylib" --lib "$out/libsable_objc.dylib" --lib "$out/libsable_system.dylib" > "$out/$engine-$ctor.txt"
      printf 'ObjC root class, scalar instance/class dispatch, nil and selectors OK\n' | cmp - "$out/$engine-$ctor.txt"
    done
    if target/release/sable run "$out/objc-2.macho" "$engine" --lib "$out/libsable_other.dylib" --lib "$out/libsable_objc.dylib" --lib "$out/libsable_system.dylib" > "$out/$engine-unknown.txt" 2> "$out/$engine-unknown.stderr"; then
      echo 'Unknown nonnil selector incorrectly succeeded' >&2; exit 1
    else
      code=$?; [[ $code == 125 ]]
      grep -Fq 'unknown ObjC selector for receiver' "$out/$engine-unknown.stderr"
    fi
  done
done
echo 'Static scalar ObjC2 dispatch, nesting, ABI, constructor access and explicit rejection passed.'
