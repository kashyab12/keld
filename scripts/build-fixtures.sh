#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p artifacts
host="$(rustc -vV | sed -n 's/^host: //p')"
linker="${SABLE_LD:-$(rustc --print sysroot)/lib/rustlib/$host/bin/rust-lld}"
for name in hello compute checksum; do
  clang -target arm64-apple-ios17.0-simulator \
    -c "modules/sable/fixtures/$name.s" -o "artifacts/$name.o"
  "$linker" -flavor darwin -arch arm64 \
    -platform_version ios-simulator 17.0 17.0 -e _start \
    -no_uuid -no_fixup_chains "artifacts/$name.o" -o "artifacts/$name.macho"
done
clang -target arm64-apple-ios17.0-simulator -O1 -ffreestanding \
  -fno-stack-protector -fno-vectorize -fno-slp-vectorize \
  -c modules/sable/fixtures/workload.c -o artifacts/workload.o
clang -target arm64-apple-ios17.0-simulator \
  -c modules/sable/fixtures/workload-start.s -o artifacts/workload-start.o
"$linker" -flavor darwin -arch arm64 \
  -platform_version ios-simulator 17.0 17.0 -e _start -no_uuid -no_fixup_chains \
  artifacts/workload.o artifacts/workload-start.o -o artifacts/workload.macho
