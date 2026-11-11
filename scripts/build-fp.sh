#!/usr/bin/env bash
# Build the original HFA/FP fixture through the headerless LLVM path (both -O0 and -Os).
# With SABLE_FP_SDK=1 on a Mac with Xcode, also build SDK-linked copies into artifacts/
# (never committed). Codegen flags deliberately allow vectorization: this fixture
# measures what ordinary clang emits.
set -euo pipefail
cd "$(dirname "$0")/.."
out=artifacts/fp
mkdir -p "$out"
host="$(rustc -vV | sed -n 's/^host: //p')"
linker="${SABLE_LD:-$(rustc --print sysroot)/lib/rustlib/$host/bin/rust-lld}"
target=(-target arm64-apple-ios17.0-simulator -Wno-incompatible-sysroot)
link=(-flavor darwin -arch arm64 -platform_version ios-simulator 17.0 17.0)
clang "${target[@]}" -c modules/sable/fixtures/dynamic-system.s -o "$out/system.o"
"$linker" "${link[@]}" -dylib -current_version 1.0 -compatibility_version 1.0 -install_name /usr/lib/libSystem.B.dylib "$out/system.o" -o "$out/libsable_system.dylib"
for opt in O0 Os; do
  clang "${target[@]}" "-$opt" -ffreestanding -fno-stack-protector -c modules/sable/fixtures/fp/hfa.c -o "$out/hfa-$opt.o"
  "$linker" "${link[@]}" -e _main "$out/hfa-$opt.o" "$out/libsable_system.dylib" -o "$out/hfa-$opt.macho"
  if [[ "${SABLE_FP_SDK:-0}" == 1 ]]; then
    sdk="$(DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}" xcrun --sdk iphonesimulator --show-sdk-path)"
    DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}" xcrun clang -target arm64-apple-ios17.0-simulator -isysroot "$sdk" "-$opt" -fno-stack-protector modules/sable/fixtures/fp/hfa.c -o "$out/hfa-sdk-$opt.macho"
  fi
done
