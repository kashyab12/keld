#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p artifacts/dynamic
host="$(rustc -vV | sed -n 's/^host: //p')"
linker="${SABLE_LD:-$(rustc --print sysroot)/lib/rustlib/$host/bin/rust-lld}"
flags=(-target arm64-apple-ios17.0-simulator -O1 -ffreestanding -fno-stack-protector -fno-vectorize -fno-slp-vectorize)
link=(-flavor darwin -arch arm64 -platform_version ios-simulator 17.0 17.0 -no_fixup_chains)
clang "${flags[@]}" -c modules/sable/fixtures/dynamic-helper.c -o artifacts/dynamic/helper.o
"$linker" "${link[@]}" -dylib -install_name @loader_path/libsable_helper.dylib -current_version 1.0 -compatibility_version 1.0 artifacts/dynamic/helper.o -o artifacts/dynamic/libsable_helper.dylib
clang "${flags[@]}" -c modules/sable/fixtures/dynamic-math.c -o artifacts/dynamic/math.o
"$linker" "${link[@]}" -dylib -install_name @loader_path/libsable_math.dylib -current_version 1.0 -compatibility_version 1.0 artifacts/dynamic/math.o artifacts/dynamic/libsable_helper.dylib -o artifacts/dynamic/libsable_math.dylib
clang "${flags[@]}" -c modules/sable/fixtures/dynamic-system.s -o artifacts/dynamic/system.o
"$linker" "${link[@]}" -dylib -install_name /usr/lib/libSystem.B.dylib -current_version 1.0 -compatibility_version 1.0 artifacts/dynamic/system.o -o artifacts/dynamic/libsable_system.dylib
clang "${flags[@]}" -c modules/sable/fixtures/dynamic-start.s -o artifacts/dynamic/start.o
for rounds in 8 2000 200000; do
  clang "${flags[@]}" -DSABLE_ROUNDS="$rounds" -c modules/sable/fixtures/dynamic-main.c -o artifacts/dynamic/main.o
  "$linker" "${link[@]}" -e _start artifacts/dynamic/main.o artifacts/dynamic/start.o artifacts/dynamic/libsable_math.dylib artifacts/dynamic/libsable_system.dylib -o "artifacts/dynamic/dynamic-$rounds.macho"
done
