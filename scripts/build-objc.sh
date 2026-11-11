#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
host="$(rustc -vV | sed -n 's/^host: //p')"
linker="${SABLE_LD:-$(rustc --print sysroot)/lib/rustlib/$host/bin/rust-lld}"
flags=(-target arm64-apple-ios17.0-simulator -O1 -ffreestanding -fno-stack-protector -fno-vectorize -fno-slp-vectorize -Wno-incompatible-sysroot -Wno-cast-of-sel-type -Wno-incomplete-implementation)
mkdir -p artifacts/objc
objc_flags=(-fno-objc-arc -fno-objc-exceptions)
# Older Clang emits absolute lists by default and predates this switch. Probe
# the compiler, then require the loader to validate the actual linked format.
if clang "${flags[@]}" -fno-objc-relative-method-lists -c -x objective-c /dev/null -o artifacts/objc/format-probe.o 2> artifacts/objc/compiler-profile.log; then
  objc_flags+=(-fno-objc-relative-method-lists)
elif [[ "$(< artifacts/objc/compiler-profile.log)" == *"unknown argument: '-fno-objc-relative-method-lists'"* ]]; then
  echo 'Compiler predates the relative-list switch; final absolute metadata will be checked.'
else
  cat artifacts/objc/compiler-profile.log >&2; exit 1
fi
clang --version | head -1
for mode in chained legacy; do
  out="artifacts/objc/$mode"; mkdir -p "$out"
  link=(-flavor darwin -arch arm64 -platform_version ios-simulator 17.0 17.0)
  if [[ "$mode" == chained ]]; then link+=(-fixup_chains -init_offsets); else link+=(-no_fixup_chains); fi
  clang "${flags[@]}" -c modules/sable/fixtures/dynamic-system.s -o "$out/system.o"
  system=("$out/system.o")
  if [[ "$mode" == legacy ]]; then
    clang "${flags[@]}" -c modules/sable/fixtures/init-binder.s -o "$out/binder.o"
    system+=("$out/binder.o")
  fi
  "$linker" "${link[@]}" -dylib -install_name /usr/lib/libSystem.B.dylib "${system[@]}" -o "$out/libsable_system.dylib"
  clang "${flags[@]}" -c modules/sable/fixtures/objc-runtime.s -o "$out/runtime.o"
  "$linker" "${link[@]}" -dylib -install_name /usr/lib/libobjc.A.dylib "$out/runtime.o" -o "$out/libsable_objc.dylib"
  clang "${flags[@]}" "${objc_flags[@]}" -c modules/sable/fixtures/objc-other.m -o "$out/other.o"
  "$linker" "${link[@]}" -no_objc_relative_method_lists -dylib -install_name @loader_path/libsable_other.dylib "$out/other.o" "$out/libsable_objc.dylib" "$out/libsable_system.dylib" -o "$out/libsable_other.dylib"
  clang "${flags[@]}" -c modules/sable/fixtures/objc-abi.s -o "$out/abi.o"
  clang "${flags[@]}" -c modules/sable/fixtures/objc-start.s -o "$out/start.o"
  for ctor in 0 1 2; do
    clang "${flags[@]}" "${objc_flags[@]}" -DOBJC_CTOR="$((ctor==1))" -DOBJC_UNKNOWN="$((ctor==2))" -c modules/sable/fixtures/objc-root.m -o "$out/main.o"
    "$linker" "${link[@]}" -no_objc_relative_method_lists -e _start "$out/start.o" "$out/main.o" "$out/abi.o" "$out/libsable_other.dylib" "$out/libsable_objc.dylib" "$out/libsable_system.dylib" -o "$out/objc-$ctor.macho"
  done
done
