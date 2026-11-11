# Native string and ownership references

These original sources observe Apple's installed runtime locally. They are not
Linux runtime implementations or passing Sable ownership probes. The minimal
source is ../minimal.m. Curated outcomes, failed attempts and cleanup are in
[evidence](../../../../../evidence/strings/research-native-summary.json).

Build locally with an installed Xcode SDK. For example, with a private output
directory and SDK/target selected explicitly:

```sh
xcrun clang -target arm64-apple-macos14 -O1 -fno-objc-arc -dynamiclib literal-image.m -framework Foundation -Wl,-install_name,@loader_path/libliteral_probe.dylib -o /private/output/libliteral_probe.dylib
xcrun clang -target arm64-apple-macos14 -O1 -fobjc-arc constants.m /private/output/libliteral_probe.dylib -framework Foundation -o /private/output/constants
xcrun clang -target arm64-apple-macos14 -O1 -fno-objc-arc ownership.m ownership-abi.S -framework Foundation -o /private/output/ownership
```

The observed iOS build used arm64-apple-ios17-simulator and the installed
iphonesimulator sysroot, with -fno-stack-protector -fno-vectorize
-fno-slp-vectorize. Runs used one private owned Simulator set, then deleted its
only device and verified zero remained. No Apple binary or SDK is distributed.
The commands above are reproduction instructions, not a newly rerun receipt.

The adjacent return producer/claim observer distinguishes optimized handoff
from the deliberate fallback ownership control. release_x20 uses x20 as input;
its native void call may clobber x0 and other caller temporaries. No native
vector/NZCV preservation or internal allocator/TLS/page behavior was established.
