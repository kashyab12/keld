#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
case "${SABLE_FIXUPS:-legacy}" in
  legacy) out=artifacts/dynamic; fixups=-no_fixup_chains ;;
  chained) out=artifacts/chained; fixups=-fixup_chains ;;
  *) echo 'SABLE_FIXUPS must be legacy or chained' >&2; exit 2 ;;
esac
mkdir -p "$out"
host="$(rustc -vV | sed -n 's/^host: //p')"
linker="${SABLE_LD:-$(rustc --print sysroot)/lib/rustlib/$host/bin/rust-lld}"
flags=(-target arm64-apple-ios17.0-simulator -O1 -ffreestanding -fno-stack-protector -fno-vectorize -fno-slp-vectorize)
link=(-flavor darwin -arch arm64 -platform_version ios-simulator 17.0 17.0 "$fixups")
clang "${flags[@]}" -c modules/sable/fixtures/dynamic-helper.c -o "$out"/helper.o
"$linker" "${link[@]}" -dylib -install_name @loader_path/libsable_helper.dylib -current_version 1.0 -compatibility_version 1.0 "$out"/helper.o -o "$out"/libsable_helper.dylib
clang "${flags[@]}" -c modules/sable/fixtures/dynamic-math.c -o "$out"/math.o
"$linker" "${link[@]}" -dylib -install_name @loader_path/libsable_math.dylib -current_version 1.0 -compatibility_version 1.0 "$out"/math.o "$out"/libsable_helper.dylib -o "$out"/libsable_math.dylib
clang "${flags[@]}" -c modules/sable/fixtures/dynamic-system.s -o "$out"/system.o
"$linker" "${link[@]}" -dylib -install_name /usr/lib/libSystem.B.dylib -current_version 1.0 -compatibility_version 1.0 "$out"/system.o -o "$out"/libsable_system.dylib
clang "${flags[@]}" -c modules/sable/fixtures/dynamic-start.s -o "$out"/start.o
for rounds in 8 2000 200000; do
  clang "${flags[@]}" -DSABLE_ROUNDS="$rounds" -c modules/sable/fixtures/dynamic-main.c -o "$out"/main.o
  "$linker" "${link[@]}" -e _start "$out"/main.o "$out"/start.o "$out"/libsable_math.dylib "$out"/libsable_system.dylib -o "$out/dynamic-$rounds.macho"
done
if [[ "${SABLE_FIXUPS:-legacy}" == chained ]]; then
  clang "${flags[@]}" -c modules/sable/fixtures/chained-numbers.s -o "$out"/numbers.o
  "$linker" "${link[@]}" -dylib -install_name @loader_path/libsable_numbers.dylib -current_version 1.0 -compatibility_version 1.0 "$out"/numbers.o -o "$out"/libsable_numbers.dylib
  clang "${flags[@]}" -c modules/sable/fixtures/chained-addend.s -o "$out"/addend.o
  "$linker" "${link[@]}" -e _start "$out"/addend.o "$out"/libsable_numbers.dylib "$out"/libsable_system.dylib -o "$out"/chained-addend.macho
fi
