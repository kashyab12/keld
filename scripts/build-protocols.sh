#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
host="$(rustc -vV | sed -n 's/^host: //p')"
linker="${SABLE_LD:-$(rustc --print sysroot)/lib/rustlib/$host/bin/rust-lld}"
flags=(-target arm64-apple-ios17.0-simulator -O1 -ffreestanding -fno-stack-protector -fno-vectorize -fno-slp-vectorize -Wno-incompatible-sysroot -Wno-protocol)
mkdir -p artifacts/protocols
objc=(-fno-objc-arc -fno-objc-exceptions)
if clang "${flags[@]}" -fno-objc-relative-method-lists -x objective-c -c /dev/null -o artifacts/protocols/probe.o 2> artifacts/protocols/compiler.log; then
  objc+=(-fno-objc-relative-method-lists)
elif [[ "$(< artifacts/protocols/compiler.log)" != *"unknown argument: '-fno-objc-relative-method-lists'"* ]]; then
  cat artifacts/protocols/compiler.log >&2; exit 1
fi
for format in absolute compact; do
 for mode in chained legacy; do
  out="artifacts/protocols/$format/$mode"; mkdir -p "$out"
  link=(-flavor darwin -arch arm64 -platform_version ios-simulator 17.0 17.0)
  if [[ "$format" == absolute ]]; then link+=(-no_objc_relative_method_lists); else link+=(-objc_relative_method_lists); fi
  if [[ "$mode" == chained ]]; then link+=(-fixup_chains -init_offsets); else link+=(-no_fixup_chains); fi
  clang "${flags[@]}" -c modules/sable/fixtures/dynamic-system.s -o "$out/system.o"
  system=("$out/system.o")
  if [[ "$mode" == legacy ]]; then
   clang "${flags[@]}" -c modules/sable/fixtures/init-binder.s -o "$out/binder.o"; system+=("$out/binder.o")
  fi
  "$linker" "${link[@]}" -dylib -install_name /usr/lib/libSystem.B.dylib "${system[@]}" -o "$out/libsable_system.dylib"
  clang "${flags[@]}" -c modules/sable/fixtures/protocol-runtime.s -o "$out/runtime.o"
  "$linker" "${link[@]}" -dylib -install_name /usr/lib/libobjc.A.dylib "$out/runtime.o" -o "$out/libsable_objc.dylib"
  clang "${flags[@]}" -c modules/sable/fixtures/objc-start.s -o "$out/start.o"
  clang "${flags[@]}" -c modules/sable/fixtures/protocol-abi.s -o "$out/abi.o"
  for ctor in 0 1; do
   clang "${flags[@]}" "${objc[@]}" -DPROTOCOL_CTOR="$ctor" -c modules/sable/fixtures/protocol-main.m -o "$out/main.o"
   "$linker" "${link[@]}" -e _start "$out/start.o" "$out/main.o" "$out/abi.o" "$out/libsable_objc.dylib" "$out/libsable_system.dylib" -o "$out/protocol-$ctor.macho"
  done
 done
done
