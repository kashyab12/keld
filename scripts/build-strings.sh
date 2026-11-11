#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
host="$(rustc -vV | sed -n 's/^host: //p')"
linker="${SABLE_LD:-$(rustc --print sysroot)/lib/rustlib/$host/bin/rust-lld}"
flags=(-target arm64-apple-ios17.0-simulator -O1 -ffreestanding -fno-stack-protector -fno-vectorize -fno-slp-vectorize -Wno-incompatible-sysroot)
objc=(-fno-objc-arc -fno-objc-exceptions)
mkdir -p artifacts/strings
if clang "${flags[@]}" -fno-objc-relative-method-lists -x objective-c -c /dev/null -o artifacts/strings/probe.o 2> artifacts/strings/compiler.log; then
 objc+=(-fno-objc-relative-method-lists)
elif [[ "$(< artifacts/strings/compiler.log)" != *"unknown argument: '-fno-objc-relative-method-lists'"* ]]; then
 cat artifacts/strings/compiler.log >&2; exit 1
fi
for format in absolute compact; do
 for mode in chained legacy; do
  out="artifacts/strings/$format/$mode";mkdir -p "$out"
  link=(-flavor darwin -arch arm64 -platform_version ios-simulator 17.0 17.0)
  if [[ "$format" == absolute ]]; then link+=(-no_objc_relative_method_lists); else link+=(-objc_relative_method_lists); fi
  if [[ "$mode" == chained ]]; then link+=(-fixup_chains -init_offsets); else link+=(-no_fixup_chains); fi
  clang "${flags[@]}" -c modules/sable/fixtures/dynamic-system.s -o "$out/system.o"
  clang "${flags[@]}" -c modules/sable/fixtures/strings/system.c -o "$out/libc.o"
  system=("$out/system.o" "$out/libc.o")
  if [[ "$mode" == legacy ]]; then
   clang "${flags[@]}" -c modules/sable/fixtures/init-binder.s -o "$out/binder.o";system+=("$out/binder.o")
  fi
  "$linker" "${link[@]}" -dylib -current_version 1.0 -compatibility_version 1.0 -install_name /usr/lib/libSystem.B.dylib "${system[@]}" -o "$out/libsable_system.dylib"
  clang "${flags[@]}" -c modules/sable/fixtures/super-runtime.s -o "$out/runtime.o"
  "$linker" "${link[@]}" -dylib -current_version 1.0 -compatibility_version 1.0 -install_name /usr/lib/libobjc.A.dylib "$out/runtime.o" -o "$out/libsable_objc.dylib"
  libs=("$out/libsable_objc.dylib" "$out/libsable_system.dylib")
  clang "${flags[@]}" "${objc[@]}" -c modules/sable/fixtures/strings/foundation.m -o "$out/foundation.o"
  "$linker" "${link[@]}" -dylib -current_version 300.0 -compatibility_version 300.0 -install_name /System/Library/Frameworks/Foundation.framework/Foundation "$out/foundation.o" "${libs[@]}" -o "$out/Foundation.dylib"
  clang "${flags[@]}" "${objc[@]}" -c modules/sable/fixtures/strings/core.m -o "$out/core.o"
  "$linker" "${link[@]}" -dylib -current_version 150.0 -compatibility_version 150.0 -install_name /System/Library/Frameworks/CoreFoundation.framework/CoreFoundation "$out/core.o" "$out/Foundation.dylib" "${libs[@]}" -o "$out/CoreFoundation.dylib"
  clang "${flags[@]}" "${objc[@]}" -c modules/sable/fixtures/strings/literal.m -o "$out/literal.o"
  "$linker" "${link[@]}" -dylib -install_name @loader_path/libsable_literals.dylib "$out/literal.o" "$out/CoreFoundation.dylib" "$out/Foundation.dylib" "$out/libsable_system.dylib" -o "$out/libsable_literals.dylib"
  clang "${flags[@]}" -c modules/sable/fixtures/objc-start.s -o "$out/start.o"
  for ctor in 0 1; do
   clang "${flags[@]}" "${objc[@]}" -DSTRING_CTOR="$ctor" -c modules/sable/fixtures/strings/client.m -o "$out/client.o"
   "$linker" "${link[@]}" -e _start "$out/start.o" "$out/client.o" "$out/libsable_literals.dylib" "$out/CoreFoundation.dylib" "$out/Foundation.dylib" "${libs[@]}" -o "$out/strings-$ctor.macho"
  done
 done
done
