#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
host="$(rustc -vV | sed -n 's/^host: //p')"
linker="${SABLE_LD:-$(rustc --print sysroot)/lib/rustlib/$host/bin/rust-lld}"
flags=(-target arm64-apple-ios17.0-simulator -O1 -ffreestanding -fno-stack-protector -fno-vectorize -fno-slp-vectorize)
for mode in chained legacy; do
  out="artifacts/tls/$mode"; mkdir -p "$out"
  link=(-flavor darwin -arch arm64 -platform_version ios-simulator 17.0 17.0)
  if [[ "$mode" == chained ]]; then link+=(-fixup_chains -init_offsets); else link+=(-no_fixup_chains); fi
  clang "${flags[@]}" -c modules/sable/fixtures/tls-system.s -o "$out/system.o"
  system=("$out/system.o")
  if [[ "$mode" == legacy ]]; then
    clang "${flags[@]}" -c modules/sable/fixtures/init-binder.s -o "$out/binder.o"
    system+=("$out/binder.o")
  fi
  "$linker" "${link[@]}" -dylib -install_name /usr/lib/libSystem.B.dylib "${system[@]}" -o "$out/libsable_system.dylib"
  for name in a b; do
    clang "${flags[@]}" -c "modules/sable/fixtures/tls-$name.c" -o "$out/$name.o"
    "$linker" "${link[@]}" -dylib -install_name "@loader_path/libtlv_$name.dylib" "$out/$name.o" "$out/libsable_system.dylib" -o "$out/libtlv_$name.dylib"
  done
  clang "${flags[@]}" -DPROBE_VECTOR=0 -c modules/sable/fixtures/tls-probe.S -o "$out/probe.o"
  clang "${flags[@]}" -c modules/sable/fixtures/tls-start.s -o "$out/start.o"
  for ctor in 0 1; do
    clang "${flags[@]}" -DTLS_CTOR="$ctor" -c modules/sable/fixtures/tls-main.c -o "$out/main.o"
    "$linker" "${link[@]}" -e _start "$out/start.o" "$out/main.o" "$out/probe.o" "$out/libtlv_a.dylib" "$out/libtlv_b.dylib" "$out/libsable_system.dylib" -o "$out/tls-$ctor.macho"
  done
done
