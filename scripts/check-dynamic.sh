#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
bash scripts/build-dynamic.sh
engine=interp
if [[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]]; then engine=jit; fi
for rounds in 8 2000 200000; do
  cargo run --quiet --release -- run "artifacts/dynamic/dynamic-$rounds.macho" "$engine" \
    --lib artifacts/dynamic/libsable_math.dylib \
    --lib artifacts/dynamic/libsable_helper.dylib \
    --lib artifacts/dynamic/libsable_system.dylib > "artifacts/dynamic/checksum-$rounds.bin"
done
python3 - <<'PY'
from pathlib import Path
expected={8:0x38afd5bab,2000:0xefe1edd5865163fb,200000:0xa7151818301809c9}
for rounds,value in expected.items():
    assert Path(f'artifacts/dynamic/checksum-{rounds}.bin').read_bytes()==value.to_bytes(8,'little')
print('All rebuilt dynamic checksums match.')
PY
