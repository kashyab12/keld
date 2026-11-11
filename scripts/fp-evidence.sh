#!/usr/bin/env bash
# Mac-only, compile-only: disassemble what Xcode clang emits (arm64-apple-ios, -O0/-Os)
# for the HFA fixture and the FirstFrame draft and count FP/SIMD forms. Nothing is
# linked or run, and the objects stay in ignored artifacts/.
set -euo pipefail
cd "$(dirname "$0")/.."
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
out=artifacts/fp/evidence
mkdir -p "$out"
sdk="$(xcrun --sdk iphoneos --show-sdk-path)"
for opt in O0 Os; do
  xcrun clang -target arm64-apple-ios17.0 -isysroot "$sdk" "-$opt" -ffreestanding -fno-stack-protector -c modules/sable/fixtures/fp/hfa.c -o "$out/hfa-$opt.o"
  xcrun clang -target arm64-apple-ios17.0 -isysroot "$sdk" "-$opt" -fobjc-arc -c modules/sable/fixtures/fp/drafts/FirstFrame-main.draft.m -o "$out/firstframe-$opt.o"
done
for o in "$out"/*.o; do xcrun llvm-objdump -d --no-show-raw-insn "$o"; done |
  awk -F'\t' 'NF>=3 && ($3 ~ /(^|[ ,\[])[bhsdqv][0-9]+([ ,.\]]|$)/ || $2 ~ /^(f|scvtf|ucvtf|movi|mov\.|sshll)/) {
    m=$2; ops=$3; gsub(/[0-9]+/,"N",ops); print m"\t"ops }' | sort | uniq -c | sort -rn
