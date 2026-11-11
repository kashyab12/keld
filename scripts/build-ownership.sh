#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
host="$(rustc -vV | sed -n 's/^host: //p')"
linker="${SABLE_LD:-$(rustc --print sysroot)/lib/rustlib/$host/bin/rust-lld}"
flags=(-target arm64-apple-ios17.0-simulator -O1 -ffreestanding -fno-stack-protector -fno-vectorize -fno-slp-vectorize -Wno-incompatible-sysroot -Wno-objc-missing-super-calls)
objc=(-fno-objc-arc -fno-objc-exceptions)
mkdir -p artifacts/ownership
if clang "${flags[@]}" -fno-objc-relative-method-lists -x objective-c -c /dev/null -o artifacts/ownership/probe.o 2> artifacts/ownership/compiler.log; then
 objc+=(-fno-objc-relative-method-lists)
elif [[ "$(< artifacts/ownership/compiler.log)" != *"unknown argument: '-fno-objc-relative-method-lists'"* ]]; then
 cat artifacts/ownership/compiler.log >&2; exit 1
fi
for format in absolute compact; do
 for mode in chained legacy; do
  out="artifacts/ownership/$format/$mode";mkdir -p "$out"
  link=(-flavor darwin -arch arm64 -platform_version ios-simulator 17.0 17.0)
  if [[ "$format" == absolute ]]; then link+=(-no_objc_relative_method_lists); else link+=(-objc_relative_method_lists); fi
  if [[ "$mode" == chained ]]; then link+=(-fixup_chains -init_offsets); else link+=(-no_fixup_chains); fi
  clang "${flags[@]}" -c modules/sable/fixtures/tls-system.s -o "$out/system.o"
  clang "${flags[@]}" -c modules/sable/fixtures/strings/system.c -o "$out/libc.o"
  system=("$out/system.o" "$out/libc.o")
  if [[ "$mode" == legacy ]]; then
   clang "${flags[@]}" -c modules/sable/fixtures/init-binder.s -o "$out/binder.o";system+=("$out/binder.o")
  fi
  "$linker" "${link[@]}" -dylib -current_version 1.0 -compatibility_version 1.0 -install_name /usr/lib/libSystem.B.dylib "${system[@]}" -o "$out/libsable_system.dylib"
  clang "${flags[@]}" -c modules/sable/fixtures/ownership/runtime.s -o "$out/runtime.o"
  "$linker" "${link[@]}" -dylib -current_version 1.0 -compatibility_version 1.0 -install_name /usr/lib/libobjc.A.dylib "$out/runtime.o" -o "$out/libsable_objc.dylib"
  libs=("$out/libsable_objc.dylib" "$out/libsable_system.dylib")
  clang "${flags[@]}" "${objc[@]}" -c modules/sable/fixtures/ownership/foundation.m -o "$out/foundation.o"
  "$linker" "${link[@]}" -dylib -current_version 300.0 -compatibility_version 300.0 -install_name /System/Library/Frameworks/Foundation.framework/Foundation "$out/foundation.o" "${libs[@]}" -o "$out/Foundation.dylib"
  clang "${flags[@]}" "${objc[@]}" -c modules/sable/fixtures/strings/core.m -o "$out/core.o"
  "$linker" "${link[@]}" -dylib -current_version 150.0 -compatibility_version 150.0 -install_name /System/Library/Frameworks/CoreFoundation.framework/CoreFoundation "$out/core.o" "$out/Foundation.dylib" "${libs[@]}" -o "$out/CoreFoundation.dylib"
  clang "${flags[@]}" -c modules/sable/fixtures/ownership/control.s -o "$out/control.o"
  for client in client observer; do
   clientflags=("${objc[@]}")
   if [[ "$client" == client ]]; then clientflags+=(-fobjc-arc); fi
   clang "${flags[@]}" "${clientflags[@]}" -S modules/sable/fixtures/ownership/$client.m -o "$out/$client.s"
   clang "${flags[@]}" "${clientflags[@]}" -c modules/sable/fixtures/ownership/$client.m -o "$out/$client.o"
   "$linker" "${link[@]}" -e _main "$out/$client.o" "$out/control.o" "$out/CoreFoundation.dylib" "$out/Foundation.dylib" "${libs[@]}" -o "$out/$client.macho"
  done
 done
done
