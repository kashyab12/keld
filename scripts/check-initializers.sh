#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
bash scripts/build-initializers.sh
cargo build --quiet --release
engines=(interp)
if [[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]]; then engines+=(jit); fi
for engine in "${engines[@]}"; do
  for mode in offsets pointers; do
    out="artifacts/initializers/$mode"
    target/release/sable run "$out/initialize.macho" "$engine" \
      --lib "$out/libsable_parent.dylib" --lib "$out/libsable_sibling.dylib" \
      --lib "$out/libsable_leaf.dylib" --lib "$out/libsable_system.dylib" > "$out/$engine.txt"
    cmp "$out/$engine.txt" modules/sable/fixtures/init-expected.txt
  done
done
echo 'Both initializer formats match the native-checked order and startup arguments.'
