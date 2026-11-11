#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
bash scripts/build-strings.sh
cargo build --quiet --release --locked
engines=(interp)
if [[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]]; then engines+=(jit); fi
for format in absolute compact; do
 for mode in chained legacy; do
  out="artifacts/strings/$format/$mode"
  python3 scripts/check-string-format.py "$format" "$out" > "$out/format-records.json"
  for engine in "${engines[@]}"; do
   for ctor in 0 1; do
    target/release/sable run "$out/strings-$ctor.macho" "$engine" --lib "$out/libsable_literals.dylib" --lib "$out/CoreFoundation.dylib" --lib "$out/Foundation.dylib" --lib "$out/libsable_system.dylib" --lib "$out/libsable_objc.dylib" > "$out/$engine-$ctor.txt"
    printf 'Constant strings, guest UTF8String and libc checks OK\n' | cmp - "$out/$engine-$ctor.txt"
   done
  done
 done
done
echo 'Original framework constant strings, inherited guest getter, scalar libc, constructor and private reset checks passed.'
