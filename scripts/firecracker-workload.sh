#!/bin/sh
set -eu
payload="$1"
work="$2"
mkdir -p "$work"
for mode in offsets pointers; do
  dir="$payload/fixtures/initializers/$mode"
  "$payload/sable" run "$dir/initialize.macho" jit \
    --lib "$dir/libsable_parent.dylib" --lib "$dir/libsable_sibling.dylib" \
    --lib "$dir/libsable_leaf.dylib" --lib "$dir/libsable_system.dylib" > "$work/$mode.txt"
  cmp "$work/$mode.txt" "$payload/expected-init.txt"
done
dir="$payload/fixtures/chained"
"$payload/sable" run "$dir/dynamic-200000.macho" jit \
  --lib "$dir/libsable_math.dylib" --lib "$dir/libsable_helper.dylib" \
  --lib "$dir/libsable_system.dylib" > "$work/checksum.bin"
cmp "$work/checksum.bin" "$payload/expected-checksum.bin"
echo SABLE_WORK_OK
