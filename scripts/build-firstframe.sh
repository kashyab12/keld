#!/usr/bin/env bash
# Build the original UI-profile providers (headerless LLVM, no Apple code) and,
# on a Mac with Xcode, the SDK-compiled FirstFrame app. The SDK app stays in
# ignored artifacts/ and is never committed or copied off this machine.
set -euo pipefail
cd "$(dirname "$0")/.."
src=modules/sable/fixtures/apps/FirstFrame
out=artifacts/firstframe
mkdir -p "$out"
host="$(rustc -vV | sed -n 's/^host: //p')"
linker="${SABLE_LD:-$(rustc --print sysroot)/lib/rustlib/$host/bin/rust-lld}"
flags=(-target arm64-apple-ios17.0-simulator -O2 -ffreestanding -fno-stack-protector -fno-vectorize -fno-slp-vectorize -Wno-incompatible-sysroot -Wno-objc-root-class -Wno-objc-missing-super-calls)
objc=(-fno-objc-arc -fno-objc-exceptions -fno-objc-convert-messages-to-runtime-calls -fobjc-runtime=ios-12.0)
link=(-flavor darwin -arch arm64 -platform_version ios-simulator 17.0 17.0 -fixup_chains -objc_relative_method_lists)
version=(-current_version 9999.0 -compatibility_version 1.0)
p="$src/providers"
clang "${flags[@]}" -c modules/sable/fixtures/dynamic-system.s -o "$out/system.o"
clang "${flags[@]}" -c "$p/libc.c" -o "$out/libc.o"
"$linker" "${link[@]}" -dylib "${version[@]}" -install_name /usr/lib/libSystem.B.dylib "$out/system.o" "$out/libc.o" -o "$out/libSystem.dylib"
clang "${flags[@]}" -c "$p/objc-runtime.s" -o "$out/objc-runtime.o"
clang "${flags[@]}" "${objc[@]}" -c "$p/NSObject.m" -o "$out/NSObject.o"
"$linker" "${link[@]}" -dylib "${version[@]}" -install_name /usr/lib/libobjc.A.dylib "$out/objc-runtime.o" "$out/NSObject.o" "$out/libSystem.dylib" -o "$out/libobjc.dylib"
clang "${flags[@]}" "${objc[@]}" -c "$p/Foundation.m" -o "$out/Foundation.o"
"$linker" "${link[@]}" -dylib "${version[@]}" -install_name /System/Library/Frameworks/Foundation.framework/Foundation "$out/Foundation.o" "$out/libobjc.dylib" "$out/libSystem.dylib" -o "$out/Foundation.dylib"
clang "${flags[@]}" -c "$p/UIKit-services.s" -o "$out/UIKit-services.o"
clang "${flags[@]}" "${objc[@]}" -c "$p/UIKit.m" -o "$out/UIKit.o"
"$linker" "${link[@]}" -dylib "${version[@]}" -install_name /System/Library/Frameworks/UIKit.framework/UIKit "$out/UIKit.o" "$out/UIKit-services.o" "$out/Foundation.dylib" "$out/libobjc.dylib" "$out/libSystem.dylib" -o "$out/UIKit.dylib"
for variant in forged:-DSABLE_FORGED_FRAMEBUFFER missing:-DSABLE_MISSING_EXPORT; do
  name="${variant%%:*}"
  clang "${flags[@]}" "${objc[@]}" "${variant#*:}" -c "$p/UIKit.m" -o "$out/UIKit-$name.o"
  "$linker" "${link[@]}" -dylib "${version[@]}" -install_name /System/Library/Frameworks/UIKit.framework/UIKit "$out/UIKit-$name.o" "$out/UIKit-services.o" "$out/Foundation.dylib" "$out/libobjc.dylib" "$out/libSystem.dylib" -o "$out/UIKit-$name.dylib"
done
if [[ "$(uname -s)" == Darwin && "${SABLE_FIRSTFRAME_SDK:-1}" == 1 ]]; then
  export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
  sdk="$(xcrun --sdk iphonesimulator --show-sdk-path)"
  for opt in O0 Os; do
    xcrun clang -target arm64-apple-ios17.0-simulator -isysroot "$sdk" -fobjc-arc "-$opt" "$src/main.m" -framework UIKit -framework Foundation -o "$out/FirstFrame-$opt"
  done
  plutil -convert binary1 -o "$out/Info.bplist" "$src/Info.plist"
  cmp "$out/Info.bplist" "$src/Info.bplist"
fi
