# Original Foundation research probes

These four original MIT sources were executed with the installed Apple SDK and
runtime on macOS ARM64 and iOS Simulator ARM64. They do not run under Sable yet.
`minimal.m` checks an autorelease pool, NSString uppercase conversion, UTF-8 and
write. `contracts.m` plus the two C dylibs independently checks constructors,
plain C TLS, pthread isolation and explicit VM operations. Its extra operations
are not evidence that minimal Foundation internally used those same calls.

On a Mac with full Xcode and the recorded iOS 27.2 runtime build 24B8117:

```sh
python3 scripts/foundation-oracle.py --output artifacts/foundation-new
```

The output directory must be new. SDK-linked binaries and complete logs stay
there, outside tracked source. The runner uses scoped DEVELOPER_DIR and creates
its own private CoreSimulator set/UUID, then shuts down/deletes only that device
and verifies zero remain. It saves the UUID immediately after creation for
recovery if the controller is forcibly interrupted. No AWS resources, global
developer settings or existing simulators are touched.

The integrated receipts are from the original private research runner; the
published runner adds output isolation and an early recovery receipt. The four
original C/Objective-C source hashes are unchanged. See the
[research findings](../../../../docs/foundation-contracts.md) and
[native receipt](../../../../evidence/foundation/receipt.json).
