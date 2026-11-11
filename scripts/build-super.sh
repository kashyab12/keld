#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
host="$(rustc -vV | sed -n 's/^host: //p')"
linker="${SABLE_LD:-$(rustc --print sysroot)/lib/rustlib/$host/bin/rust-lld}"
flags=(-target arm64-apple-ios17.0-simulator -O1 -ffreestanding -fno-stack-protector -fno-vectorize -fno-slp-vectorize -Wno-incompatible-sysroot -Wno-cast-of-sel-type)
objc=(-fno-objc-arc -fno-objc-exceptions)
mkdir -p artifacts/super
if clang "${flags[@]}" -fno-objc-relative-method-lists -x objective-c -c /dev/null -o artifacts/super/probe.o 2> artifacts/super/compiler.log; then
 objc+=(-fno-objc-relative-method-lists)
elif [[ "$(< artifacts/super/compiler.log)" != *"unknown argument: '-fno-objc-relative-method-lists'"* ]]; then
 cat artifacts/super/compiler.log >&2; exit 1
fi
for format in absolute compact; do
 for mode in chained legacy; do
  for split in 0 1; do
   out="artifacts/super/$format/$mode/$split"; mkdir -p "$out"
   link=(-flavor darwin -arch arm64 -platform_version ios-simulator 17.0 17.0)
   if [[ "$format" == absolute ]]; then link+=(-no_objc_relative_method_lists); else link+=(-objc_relative_method_lists); fi
   if [[ "$mode" == chained ]]; then link+=(-fixup_chains -init_offsets); else link+=(-no_fixup_chains); fi
   clang "${flags[@]}" -c modules/sable/fixtures/dynamic-system.s -o "$out/system.o"
   system=("$out/system.o")
   if [[ "$mode" == legacy ]]; then
    clang "${flags[@]}" -c modules/sable/fixtures/init-binder.s -o "$out/binder.o"; system+=("$out/binder.o")
   fi
   "$linker" "${link[@]}" -dylib -install_name /usr/lib/libSystem.B.dylib "${system[@]}" -o "$out/libsable_system.dylib"
   clang "${flags[@]}" -c modules/sable/fixtures/super-runtime.s -o "$out/runtime.o"
   "$linker" "${link[@]}" -dylib -install_name /usr/lib/libobjc.A.dylib "$out/runtime.o" -o "$out/libsable_objc.dylib"
   clang "${flags[@]}" -c modules/sable/fixtures/super-abi.s -o "$out/abi.o"
   clang "${flags[@]}" -c modules/sable/fixtures/objc-start.s -o "$out/start.o"
   libs=("$out/libsable_objc.dylib" "$out/libsable_system.dylib"); extra=("$out/abi.o")
   if [[ "$split" == 1 ]]; then
    clang "${flags[@]}" "${objc[@]}" -DSUPER_SPLIT=1 -DSUPER_BASE_ONLY=1 -c modules/sable/fixtures/super-main.m -o "$out/base.o"
    "$linker" "${link[@]}" -dylib -install_name @loader_path/libsable_base.dylib "$out/base.o" "$out/abi.o" "${libs[@]}" -o "$out/libsable_base.dylib"
    extra=("$out/libsable_base.dylib")
   fi
   for ctor in 0 1 2 3; do
    if [[ "$ctor" == 3 && "$split" == 1 ]]; then continue; fi
    clang "${flags[@]}" "${objc[@]}" -DSUPER_SPLIT="$split" -DSUPER_BASE_ONLY=0 -DSUPER_CTOR="$((ctor==1))" -DSUPER_DEEP="$((ctor==2))" -DSUPER_PROTOCOL="$((ctor==3))" -c modules/sable/fixtures/super-main.m -o "$out/main.o"
    "$linker" "${link[@]}" -e _start "$out/start.o" "$out/main.o" "${extra[@]}" "${libs[@]}" -o "$out/super-$ctor.macho"
   done
  done
 done
done
