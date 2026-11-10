#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
host="$(rustc -vV | sed -n 's/^host: //p')"
linker="${SABLE_LD:-$(rustc --print sysroot)/lib/rustlib/$host/bin/rust-lld}"
flags=(-target arm64-apple-ios17.0-simulator -O1 -ffreestanding -fno-stack-protector -fno-vectorize -fno-slp-vectorize)
for mode in offsets pointers; do
  out="artifacts/initializers/$mode"
  mkdir -p "$out"
  link=(-flavor darwin -arch arm64 -platform_version ios-simulator 17.0 17.0)
  if [[ "$mode" == offsets ]]; then link+=(-fixup_chains -init_offsets); else link+=(-no_fixup_chains); fi
  clang "${flags[@]}" -c modules/sable/fixtures/dynamic-system.s -o "$out/system.o"
  system=("$out/system.o")
  if [[ "$mode" == pointers ]]; then
    clang "${flags[@]}" -c modules/sable/fixtures/init-binder.s -o "$out/binder.o"
    system+=("$out/binder.o")
  fi
  "$linker" "${link[@]}" -dylib -install_name /usr/lib/libSystem.B.dylib "${system[@]}" -o "$out/libsable_system.dylib"
  for name in leaf parent sibling main; do
    clang "${flags[@]}" -c "modules/sable/fixtures/init-$name.c" -o "$out/$name.o"
  done
  "$linker" "${link[@]}" -dylib -install_name @loader_path/libsable_leaf.dylib "$out/leaf.o" "$out/libsable_system.dylib" -o "$out/libsable_leaf.dylib"
  for name in parent sibling; do
    "$linker" "${link[@]}" -dylib -install_name "@loader_path/libsable_$name.dylib" "$out/$name.o" "$out/libsable_leaf.dylib" "$out/libsable_system.dylib" -o "$out/libsable_$name.dylib"
  done
  clang "${flags[@]}" -c modules/sable/fixtures/init-start.s -o "$out/start.o"
  "$linker" "${link[@]}" -e _start "$out/main.o" "$out/start.o" "$out/libsable_parent.dylib" "$out/libsable_sibling.dylib" "$out/libsable_leaf.dylib" "$out/libsable_system.dylib" -o "$out/initialize.macho"
done
