#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
host="$(rustc -vV | sed -n 's/^host: //p')"
linker="${SABLE_LD:-$(rustc --print sysroot)/lib/rustlib/$host/bin/rust-lld}"
flags=(-target arm64-apple-ios17.0-simulator -O1 -ffreestanding -fno-stack-protector -fno-vectorize -fno-slp-vectorize -Wno-incompatible-sysroot -Wno-cast-of-sel-type)
objc=(-fno-objc-arc -fno-objc-exceptions)
mkdir -p artifacts/payload
if clang "${flags[@]}" -fno-objc-relative-method-lists -x objective-c -c /dev/null -o artifacts/payload/probe.o 2> artifacts/payload/compiler.log; then
 objc+=(-fno-objc-relative-method-lists)
elif [[ "$(< artifacts/payload/compiler.log)" != *"unknown argument: '-fno-objc-relative-method-lists'"* ]]; then
 cat artifacts/payload/compiler.log >&2; exit 1
fi
for format in absolute compact; do
 for mode in chained legacy; do
  for profile in 0 1 2 3; do
   case "$profile" in
    0) rw=1; cw=1 ;;
    1) rw=0; cw=1 ;;
    2) rw=7; cw=0 ;;
    3) rw=1; cw=6 ;;
   esac
   for split in 0 1; do
   out="artifacts/payload/$format/$mode/$profile/$split"; mkdir -p "$out"
   link=(-flavor darwin -arch arm64 -platform_version ios-simulator 17.0 17.0)
   if [[ "$format" == absolute ]]; then link+=(-no_objc_relative_method_lists); else link+=(-objc_relative_method_lists); fi
   if [[ "$mode" == chained ]]; then link+=(-fixup_chains -init_offsets); else link+=(-no_fixup_chains); fi
   clang "${flags[@]}" -c modules/sable/fixtures/tls-system.s -o "$out/system.o"
   system=("$out/system.o")
   if [[ "$mode" == legacy ]]; then
    clang "${flags[@]}" -c modules/sable/fixtures/init-binder.s -o "$out/binder.o"; system+=("$out/binder.o")
   fi
   "$linker" "${link[@]}" -dylib -install_name /usr/lib/libSystem.B.dylib "${system[@]}" -o "$out/libsable_system.dylib"
   clang "${flags[@]}" -c modules/sable/fixtures/instance-runtime.s -o "$out/runtime.o"
   "$linker" "${link[@]}" -dylib -install_name /usr/lib/libobjc.A.dylib "$out/runtime.o" -o "$out/libsable_objc.dylib"
   clang "${flags[@]}" -c modules/sable/fixtures/protocol-abi.s -o "$out/abi.o"
   clang "${flags[@]}" -c modules/sable/fixtures/objc-start.s -o "$out/start.o"
   libs=("$out/libsable_objc.dylib" "$out/libsable_system.dylib"); extra=("$out/abi.o")
   if [[ "$split" == 1 ]]; then
    clang "${flags[@]}" "${objc[@]}" -DROOT_WORDS="$rw" -DCHILD_WORDS="$cw" -DPAYLOAD_SPLIT=1 -DPAYLOAD_BASE_ONLY=1 -c modules/sable/fixtures/payload-main.m -o "$out/base.o"
    "$linker" "${link[@]}" -dylib -install_name @loader_path/libsable_base.dylib "$out/base.o" "$out/abi.o" "${libs[@]}" -o "$out/libsable_base.dylib"
    extra=("$out/libsable_base.dylib")
   fi
   for ctor in 0 1; do
    clang "${flags[@]}" "${objc[@]}" -DROOT_WORDS="$rw" -DCHILD_WORDS="$cw" -DPAYLOAD_SPLIT="$split" -DPAYLOAD_BASE_ONLY=0 -DPAYLOAD_CTOR="$((ctor==1))" -c modules/sable/fixtures/payload-main.m -o "$out/main.o"
    "$linker" "${link[@]}" -e _start "$out/start.o" "$out/main.o" "${extra[@]}" "${libs[@]}" -o "$out/payload-$ctor.macho"
   done
  done
 done
done
done
