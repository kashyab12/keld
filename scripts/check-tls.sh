#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
bash scripts/build-tls.sh
cargo build --quiet --release
engines=(interp)
if [[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]]; then engines+=(jit); fi
for engine in "${engines[@]}"; do
  for mode in chained legacy; do
    out="artifacts/tls/$mode"
    for ctor in 0 1; do
      target/release/sable run "$out/tls-$ctor.macho" "$engine" \
        --lib "$out/libtlv_a.dylib" --lib "$out/libtlv_b.dylib" --lib "$out/libsable_system.dylib" > "$out/$engine-$ctor.txt"
      printf 'TLV scalar ABI, templates, two images, stable addresses OK\n' | cmp - "$out/$engine-$ctor.txt"
    done
  done
done
echo 'Scalar TLV templates, ABI and constructor/main access passed in both fixup formats.'
