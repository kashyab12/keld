#!/usr/bin/env python3
"""Build and externally drive the original UIKit/Metal app on an owned simulator.

Native oracle only: successful XCTest results never mean Sable supports apps.
No global Xcode selection, device settings or existing simulator is changed.
Build products, Apple test support, xcresult and logs stay outside Git.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import plistlib
import signal
import subprocess
import uuid

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "modules/sable/fixtures/apps/Notes"
ENV = dict(os.environ, DEVELOPER_DIR=os.environ.get("DEVELOPER_DIR", "/Applications/Xcode.app/Contents/Developer"))


def command(args, **kwargs):
    return subprocess.run(args, env=ENV, capture_output=True, timeout=60, check=True, **kwargs).stdout


def project(folder):
    """Generate an ordinary Xcode project using only Python's plist library."""
    objects = {}

    def obj(isa, **fields):
        key = f"{len(objects) + 1:024X}"
        objects[key] = {"isa": isa, **fields}
        return key

    app_file = obj("PBXFileReference", lastKnownFileType="sourcecode.c.objc", path=str(SOURCE / "main.m"), sourceTree="<absolute>")
    test_file = obj("PBXFileReference", lastKnownFileType="sourcecode.swift", path=str(SOURCE / "InteractionTests.swift"), sourceTree="<absolute>")
    app_product = obj("PBXFileReference", explicitFileType="wrapper.application", path="SableNotes.app", sourceTree="BUILT_PRODUCTS_DIR")
    test_product = obj("PBXFileReference", explicitFileType="wrapper.cfbundle", path="InteractionTests.xctest", sourceTree="BUILT_PRODUCTS_DIR")
    app_source = obj("PBXBuildFile", fileRef=app_file)
    test_source = obj("PBXBuildFile", fileRef=test_file)
    app_phase = obj("PBXSourcesBuildPhase", buildActionMask=2147483647, files=[app_source], runOnlyForDeploymentPostprocessing=0)
    test_phase = obj("PBXSourcesBuildPhase", buildActionMask=2147483647, files=[test_source], runOnlyForDeploymentPostprocessing=0)
    group = obj("PBXGroup", children=[app_file, test_file, app_product, test_product], sourceTree="<group>")
    common = dict(SDKROOT="iphonesimulator", IPHONEOS_DEPLOYMENT_TARGET="17.0", CLANG_ENABLE_OBJC_ARC="YES",
                  CLANG_ENABLE_MODULES="YES", SWIFT_VERSION="5.0", TARGETED_DEVICE_FAMILY="1", CODE_SIGNING_ALLOWED="NO",
                  SUPPORTED_PLATFORMS="iphonesimulator", GENERATE_INFOPLIST_FILE="YES", ENABLE_USER_SCRIPT_SANDBOXING="YES")

    def configs(settings):
        config = obj("XCBuildConfiguration", name="Release", buildSettings=settings)
        return obj("XCConfigurationList", buildConfigurations=[config], defaultConfigurationIsVisible=0, defaultConfigurationName="Release")

    global_configs = configs(common)
    info = folder / "Notes-Info.plist"
    info.write_bytes(plistlib.dumps({"UIApplicationSceneManifest": {
        "UIApplicationSupportsMultipleScenes": False,
        "UISceneConfigurations": {"UIWindowSceneSessionRoleApplication": [
            {"UISceneConfigurationName": "Default", "UISceneDelegateClassName": "SceneDelegate"}]}}}))
    app_configs = configs(dict(PRODUCT_NAME="SableNotes", PRODUCT_BUNDLE_IDENTIFIER="org.keld.sable.notes",
                              INFOPLIST_FILE=str(info),
                              INFOPLIST_KEY_UILaunchScreen_Generation="YES", INFOPLIST_KEY_UIRequiresFullScreen="YES",
                              OTHER_LDFLAGS=["-framework", "UIKit", "-framework", "MetalKit", "-framework", "Metal"]))
    test_configs = configs(dict(PRODUCT_NAME="InteractionTests", PRODUCT_BUNDLE_IDENTIFIER="org.keld.sable.notes.tests",
                               TEST_TARGET_NAME="SableNotes", LD_RUNPATH_SEARCH_PATHS=["$(inherited)", "@executable_path/Frameworks", "@loader_path/Frameworks"]))
    app = obj("PBXNativeTarget", name="SableNotes", buildConfigurationList=app_configs, buildPhases=[app_phase],
              buildRules=[], dependencies=[], productName="SableNotes", productReference=app_product, productType="com.apple.product-type.application")
    dependency = obj("PBXTargetDependency", target=app)
    tests = obj("PBXNativeTarget", name="InteractionTests", buildConfigurationList=test_configs, buildPhases=[test_phase],
                buildRules=[], dependencies=[dependency], productName="InteractionTests", productReference=test_product, productType="com.apple.product-type.bundle.ui-testing")
    root = obj("PBXProject", attributes={"LastUpgradeCheck": "2700", "TargetAttributes": {tests: {"TestTargetID": app}}},
               buildConfigurationList=global_configs, compatibilityVersion="Xcode 14.0", developmentRegion="en", knownRegions=["en", "Base"],
               mainGroup=group, projectDirPath="", projectRoot="", targets=[app, tests])
    path = folder / "SableNotes.xcodeproj"
    path.mkdir()
    (path / "project.pbxproj").write_bytes(plistlib.dumps(dict(archiveVersion="1", classes={}, objectVersion="56", objects=objects, rootObject=root)))
    schemes = path / "xcshareddata/xcschemes"
    schemes.mkdir(parents=True)
    reference = lambda identifier, name: f'<BuildableReference BuildableIdentifier="primary" BlueprintIdentifier="{identifier}" BuildableName="{name}" BlueprintName="{name.split(".")[0]}" ReferencedContainer="container:SableNotes.xcodeproj"/>'
    (schemes / "SableNotes.xcscheme").write_text(f'''<?xml version="1.0" encoding="UTF-8"?>
<Scheme LastUpgradeVersion="2700" version="1.3">
<BuildAction parallelizeBuildables="YES" buildImplicitDependencies="YES"><BuildActionEntries>
<BuildActionEntry buildForTesting="YES" buildForRunning="YES" buildForProfiling="NO" buildForArchiving="NO" buildForAnalyzing="YES">{reference(app, "SableNotes.app")}</BuildActionEntry>
<BuildActionEntry buildForTesting="YES" buildForRunning="NO" buildForProfiling="NO" buildForArchiving="NO" buildForAnalyzing="YES">{reference(tests, "InteractionTests.xctest")}</BuildActionEntry>
</BuildActionEntries></BuildAction>
<TestAction buildConfiguration="Release" shouldUseLaunchSchemeArgsEnv="YES"><Testables><TestableReference skipped="NO">{reference(tests, "InteractionTests.xctest")}</TestableReference></Testables></TestAction>
<LaunchAction buildConfiguration="Release" launchStyle="0" useCustomWorkingDirectory="NO" ignoresPersistentStateOnLaunch="NO" debugDocumentVersioning="YES" allowLocationSimulation="YES"><BuildableProductRunnable runnableDebuggingMode="0">{reference(app, "SableNotes.app")}</BuildableProductRunnable></LaunchAction>
</Scheme>
''')
    return path


def sim(*args, check=True):
    return subprocess.run(["xcrun", "simctl", *args], env=ENV, capture_output=True, text=True, timeout=60, check=check)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path, help="New private artifact directory, outside tracked source")
    parser.add_argument("--record", action="store_true", help="Record this owned native reference device during external UI tests")
    options = parser.parse_args()
    folder = options.output.resolve()
    # xcresult bundles include Apple test support and diagnostics; never publish them wholesale.
    if folder.is_relative_to(ROOT) and not folder.is_relative_to(ROOT / "artifacts"):
        parser.error("use artifacts/ or a private directory outside the repository")
    folder.mkdir(parents=True, exist_ok=False)
    receipt = dict(started_at=datetime.datetime.now(datetime.timezone.utc).isoformat(), kind="native_apple_app_oracle",
                   source_sha256={p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(SOURCE.iterdir()) if p.is_file()},
                   driver_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                   native_oracle_passed=False, sable_app_support_proven=False, simulator_window_requested=False,
                   gpu_mode="native host graphics; not a no-GPU test", devices_created=[])
    device = None
    recorder = None
    try:
        receipt["xcode"] = command(["xcodebuild", "-version"]).decode().strip()
        receipt["host_arch"] = command(["uname", "-m"]).decode().strip()
        path = project(folder)
        base = ["xcodebuild", "-project", str(path), "-scheme", "SableNotes", "-configuration", "Release", "-derivedDataPath", str(folder / "build"),
                "-parallel-testing-enabled", "NO", "CODE_SIGNING_ALLOWED=NO"]
        with (folder / "build.log").open("w") as log:
            built = subprocess.run(base + ["-destination", "generic/platform=iOS Simulator", "build-for-testing"], env=ENV, stdout=log, stderr=subprocess.STDOUT, timeout=300)
        receipt["build_exit"] = built.returncode
        built.check_returncode()
        executable = folder / "build/Build/Products/Release-iphonesimulator/SableNotes.app/SableNotes"
        receipt["executable_sha256"] = hashlib.sha256(executable.read_bytes()).hexdigest()
        # Xcode's generic destination builds a universal executable. Give the
        # ARM64-only Sable loader its actual ARM64 slice, not the fat container.
        arm64 = folder / "SableNotes.arm64.macho"
        command(["xcrun", "lipo", str(executable), "-thin", "arm64", "-output", str(arm64)])
        receipt["arm64_executable_sha256"] = hashlib.sha256(arm64.read_bytes()).hexdigest()
        # Observe Sable's real first failure using this app. A loader rejection is
        # a failed app workflow, even when rejection is safe and expected today.
        sable = ROOT / "target/release/sable"
        if sable.exists():
            tried = subprocess.run([str(sable), "run", str(arm64), "interp", "1000000"], capture_output=True, timeout=30)
            receipt["sable_probe"] = dict(exit=tried.returncode, stderr=tried.stderr.decode(errors="replace"), stdout_hex=tried.stdout.hex(),
                                          binary_sha256=hashlib.sha256(sable.read_bytes()).hexdigest(),
                                          completed_interactions=0, app_workflow_status="unsupported" if tried.returncode else "unverified",
                                          scope="actual app executable attempted; no UI driver in Sable")
        else:
            receipt["sable_probe"] = dict(app_workflow_status="not_run", reason="build Sable with cargo build --release first")
        runtimes = json.loads(sim("list", "runtimes", "--json").stdout)["runtimes"]
        runtime = next(r for r in runtimes if r["isAvailable"] and r.get("platform") == "iOS" and any(t["identifier"] == "com.apple.CoreSimulator.SimDeviceType.iPhone-17" for t in r["supportedDeviceTypes"]))
        receipt["runtime"] = {k: runtime[k] for k in ["identifier", "version", "buildversion"]}
        # Xcode's public destination interface discovers the default device set.
        # Create exactly one fresh UUID there; never use 'booted' or shutdown all.
        device = sim("create", "Sable app check " + str(uuid.uuid4())[:8], "com.apple.CoreSimulator.SimDeviceType.iPhone-17", runtime["identifier"]).stdout.strip()
        receipt["devices_created"] = [device]
        (folder / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
        sim("boot", device)
        with (folder / "boot.log").open("w") as log:
            boot = subprocess.run(["xcrun", "simctl", "bootstatus", device, "-b"], env=ENV, stdout=log, stderr=subprocess.STDOUT, timeout=180)
        boot.check_returncode()
        if options.record:
            video = folder / "native-reference.mp4"
            with (folder / "recording.log").open("wb") as log:
                recorder = subprocess.Popen(["xcrun", "simctl", "io", device, "recordVideo", "--codec=h264", str(video)],
                                            env=ENV, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT)
        with (folder / "test.log").open("w") as log:
            tested = subprocess.run(base + ["-destination", "platform=iOS Simulator,id=" + device, "-destination-timeout", "30",
                                            "-resultBundlePath", str(folder / "tests.xcresult"), "test-without-building"],
                                    env=ENV, stdout=log, stderr=subprocess.STDOUT, timeout=600)
        receipt["test_exit"] = tested.returncode
        if (folder / "tests.xcresult").exists():
            summary = json.loads(command(["xcrun", "xcresulttool", "get", "test-results", "summary", "--path", str(folder / "tests.xcresult"), "--compact"]))
            (folder / "test-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
            receipt["tests"] = {k: summary.get(k) for k in ["passedTests", "failedTests", "skippedTests", "totalTestCount", "finishTime", "startTime"]}
            receipt["native_oracle_passed"] = tested.returncode == 0 and summary.get("totalTestCount") == 3 and summary.get("passedTests") == 3 and summary.get("failedTests") == 0 and summary.get("skippedTests") == 0
        tested.check_returncode()
    except Exception as error:
        receipt["error"] = type(error).__name__ + ": " + str(error)
    finally:
        if recorder:
            recorder.send_signal(signal.SIGINT)
            try:
                recorder.wait(timeout=30)
            except subprocess.TimeoutExpired:
                recorder.kill(); recorder.wait()
                receipt["recording_error"] = "recorder did not finalize within 30 seconds"
            video = folder / "native-reference.mp4"
            receipt["recording"] = dict(kind="native_apple_reference_not_sable", exit=recorder.returncode,
                                        bytes=video.stat().st_size if video.exists() else 0,
                                        sha256=hashlib.sha256(video.read_bytes()).hexdigest() if video.exists() else None)
        if device:
            sim("shutdown", device, check=False)
            deleted = sim("delete", device, check=False)
            remaining = json.loads(sim("list", "devices", "--json").stdout)["devices"]
            receipt["cleanup"] = dict(delete_exit=deleted.returncode, owned_device_remaining=any(d["udid"] == device for v in remaining.values() for d in v))
        (folder / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt, indent=2))
    if not receipt["native_oracle_passed"] or receipt.get("cleanup", {}).get("owned_device_remaining", True) or receipt.get("cleanup", {}).get("delete_exit") != 0:
        raise SystemExit(1)
    if options.record and (receipt.get("recording_error") or receipt.get("recording", {}).get("bytes", 0) == 0):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
