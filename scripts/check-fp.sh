#!/usr/bin/env bash
# Rebuild the original HFA fixture with the host LLVM and require byte-exact output
# from every engine available here.
set -euo pipefail
cd "$(dirname "$0")/.."
bash scripts/build-fp.sh
cargo build --quiet --release --locked
sable="${CARGO_TARGET_DIR:-target}/release/sable"
engines=(interp)
if [[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]]; then engines+=(jit); fi
for engine in "${engines[@]}"; do
  for opt in O0 Os; do
    "$sable" run "artifacts/fp/hfa-$opt.macho" "$engine" --main --lib artifacts/fp/libsable_system.dylib > "artifacts/fp/$engine-$opt.txt"
    cmp "artifacts/fp/$engine-$opt.txt" modules/sable/fixtures/fp/hfa-expected.txt
  done
done
echo "HFA fixture byte-exact: ${engines[*]} x O0/Os."
