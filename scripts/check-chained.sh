#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
SABLE_FIXUPS=chained bash scripts/build-dynamic.sh
cargo build --quiet --release
engines=(interp)
if [[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]]; then engines+=(jit); fi
for engine in "${engines[@]}"; do
  for rounds in 8 2000 200000; do
    target/release/sable run "artifacts/chained/dynamic-$rounds.macho" "$engine" \
      --lib artifacts/chained/libsable_math.dylib \
      --lib artifacts/chained/libsable_helper.dylib \
      --lib artifacts/chained/libsable_system.dylib > "artifacts/chained/$engine-$rounds.bin"
  done
  target/release/sable run artifacts/chained/chained-addend.macho "$engine" \
    --lib artifacts/chained/libsable_numbers.dylib \
    --lib artifacts/chained/libsable_system.dylib > "artifacts/chained/$engine-addend.bin"
done
python3 - <<'PY'
import platform
from pathlib import Path
engines=['interp'] + (['jit'] if platform.system()=='Linux' and platform.machine()=='x86_64' else [])
expected={8:0x38afd5bab,2000:0xefe1edd5865163fb,200000:0xa7151818301809c9,'addend':51}
for engine in engines:
    for name,value in expected.items():
        assert Path(f'artifacts/chained/{engine}-{name}.bin').read_bytes()==value.to_bytes(8,'little')
print('All four rebuilt chained fixtures match in every available engine.')
PY
