use sable::{
    engine::{Outcome, Prepared},
    macho::{Image, Segment},
};
fn fixtures(format: &str, mode: &str, ctor: u8) -> (Vec<u8>, Vec<Vec<u8>>) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("fixtures/bin/strings/{format}/{mode}"));
    let libs = [
        "libsable_literals.dylib",
        "CoreFoundation.dylib",
        "Foundation.dylib",
        "libsable_system.dylib",
        "libsable_objc.dylib",
    ]
    .map(|n| std::fs::read(dir.join(n)).unwrap());
    (
        std::fs::read(dir.join(format!("strings-{ctor}.macho"))).unwrap(),
        libs.into(),
    )
}
fn link(raw: &[u8], libs: &[Vec<u8>]) -> sable::Result<Image> {
    Image::link(raw, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>())
}
fn compare(image: &Image, budget: u64) -> sable::Result<Outcome> {
    let result = Prepared::new(image, false).and_then(|p| p.run(budget));
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    for dispatch in [false, true] {
        let other = Prepared::new_with_dispatch(image, dispatch).and_then(|p| p.run(budget));
        match (&result, &other) {
            (Ok(a), Ok(b)) => assert_eq!(a, b),
            (Err(a), Err(b)) => assert_eq!(a, b),
            _ => panic!("string mismatch {result:?}/{other:?}"),
        }
    }
    result
}
fn section(raw: &[u8], name: &str) -> usize {
    let u32at = |n| u32::from_le_bytes(raw[n..n + 4].try_into().unwrap()) as usize;
    let mut at = 32;
    for _ in 0..u32at(16) {
        if u32at(at) == 0x19 {
            for n in 0..u32at(at + 64) {
                let s = at + 72 + n * 80;
                if &raw[s..s + name.len()] == name.as_bytes()
                    && (name.len() == 16 || raw[s + name.len()] == 0)
                {
                    return s;
                }
            }
        }
        at += u32at(at + 4);
    }
    panic!("section {name}");
}
fn addr(raw: &[u8], header: usize) -> u64 {
    u64::from_le_bytes(raw[header + 32..header + 40].try_into().unwrap())
}
fn patch(image: &mut Image, a: u64, data: &[u8]) {
    for s in &mut image.segments {
        if let Some(n) = a.checked_sub(s.address).map(|v| v as usize)
            && n.checked_add(data.len())
                .is_some_and(|end| end <= s.bytes.len())
        {
            std::sync::Arc::make_mut(&mut s.bytes)[n..n + data.len()].copy_from_slice(data);
            return;
        }
    }
    panic!("bad patch");
}
#[test]
fn original_framework_getter_libc_multi_image_constants_constructor_and_private_reset() {
    for format in ["absolute", "compact"] {
        for mode in ["chained", "legacy"] {
            for ctor in [0, 1] {
                let (raw, mut libs) = fixtures(format, mode, ctor);
                let image = link(&raw, &libs).unwrap();
                let first = compare(&image, 20000).unwrap();
                assert_eq!(first.exit, 0);
                assert_eq!(
                    first.stdout,
                    b"Constant strings, guest UTF8String and libc checks OK\n"
                );
                assert_eq!(first.syscalls, 2);
                let prepared = Prepared::new(&image, false).unwrap();
                for _ in 0..3 {
                    assert_eq!(prepared.run(20000).unwrap(), first);
                }
                for budget in [0, 1, 20, 200] {
                    assert!(prepared.run(budget).unwrap_err().contains("budget"));
                    assert_eq!(prepared.run(20000).unwrap(), first);
                }
                #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                for dispatch in [false, true] {
                    let p = Prepared::new_with_dispatch(&image, dispatch).unwrap();
                    for _ in 0..3 {
                        assert_eq!(p.run(20000).unwrap(), first);
                    }
                }
                libs.reverse();
                assert_eq!(compare(&link(&raw, &libs).unwrap(), 20000).unwrap(), first);
            }
        }
    }
}
#[test]
fn malformed_constant_sections_missing_wrong_version_or_alias_frameworks_reject() {
    let (raw, libs) = fixtures("absolute", "chained", 0);
    let s = section(&raw, "__cfstring");
    for (at, value) in [(s + 40, 31u64), (s + 40, 8193), (s + 32, addr(&raw, s) + 1)] {
        let mut bad = raw.clone();
        bad[at..at + 8].copy_from_slice(&value.to_le_bytes());
        assert!(link(&bad, &libs).is_err());
    }
    for (at, value) in [(s + 52, 13u32), (s + 64, 1), (s + 48, u32::MAX)] {
        let mut bad = raw.clone();
        bad[at..at + 4].copy_from_slice(&value.to_le_bytes());
        assert!(link(&bad, &libs).is_err());
    }
    let mut bad = raw.clone();
    let mut at = 32;
    while at < bad.len() {
        let cmd = u32::from_le_bytes(bad[at..at + 4].try_into().unwrap());
        let n = u32::from_le_bytes(bad[at + 4..at + 8].try_into().unwrap()) as usize;
        if cmd == 0x19 && u32::from_le_bytes(bad[at + 68..at + 72].try_into().unwrap()) & 16 != 0 {
            let flags = u32::from_le_bytes(bad[at + 68..at + 72].try_into().unwrap()) & !16;
            bad[at + 68..at + 72].copy_from_slice(&flags.to_le_bytes());
            break;
        }
        at += n;
    }
    assert!(link(&bad, &libs).unwrap_err().contains("read-only"));
    for skip in [1, 2, 3, 4] {
        let remaining = libs
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != skip)
            .map(|(_, p)| p.clone())
            .collect::<Vec<_>>();
        assert!(link(&raw, &remaining).is_err());
    }
    let mut wrong = libs.clone();
    let mut at = 32;
    loop {
        let cmd = u32::from_le_bytes(wrong[1][at..at + 4].try_into().unwrap());
        if cmd == 0xd {
            wrong[1][at + 16..at + 20].copy_from_slice(&0u32.to_le_bytes());
            break;
        }
        at += u32::from_le_bytes(wrong[1][at + 4..at + 8].try_into().unwrap()) as usize;
    }
    assert!(
        link(&raw, &wrong)
            .unwrap_err()
            .contains("incompatible dylib version")
    );
    let mut wrong = libs.clone();
    let needle = b"CFConstantStringClassReference";
    let mut changed = 0;
    for at in 0..wrong[1].len() - needle.len() {
        if &wrong[1][at..at + needle.len()] == needle {
            wrong[1][at] = b'X';
            changed += 1;
        }
    }
    assert!(changed > 0);
    assert!(link(&raw, &wrong).is_err());
}
#[test]
fn constants_snapshot_permissions_code_aliases_and_actual_getter_are_enforced() {
    let (raw, libs) = fixtures("absolute", "chained", 0);
    let image = link(&raw, &libs).unwrap();
    let cf = addr(&raw, section(&raw, "__cfstring"));
    let bytes = u64::from_le_bytes(image.read(cf + 16, 8, 1).unwrap().try_into().unwrap());
    for a in [cf, cf + 8, cf + 16, cf + 24, bytes] {
        let mut bad = image.clone();
        patch(&mut bad, a, &[0xff]);
        assert!(Prepared::new(&bad, false).is_err());
    }
    for a in [cf, bytes] {
        let mut bad = image.clone();
        bad.extra_text.push((a, 4));
        assert!(Prepared::new(&bad, false).is_err());
        let mut bad = image.clone();
        bad.segments
            .iter_mut()
            .find(|s| a >= s.address && a < s.address + s.bytes.len() as u64)
            .unwrap()
            .protection |= 2;
        assert!(Prepared::new(&bad, false).is_err());
    }
    // Replacing the real Foundation getter with nil makes actual byte access fail.
    let getter = image
        .code_ranges()
        .flat_map(|(a, n)| (a..a + n).step_by(4))
        .find(|&a| {
            image.instruction(a).unwrap() == 0xf9400800
                && image.instruction(a + 4).ok() == Some(0xd65f03c0)
        })
        .unwrap();
    let mut bad = image.clone();
    patch(&mut bad, getter, &0xd2800000u32.to_le_bytes());
    let error = Prepared::new(&bad, false).unwrap().run(20000).unwrap_err();
    assert_eq!(error, "guest memory fault at 0x0, length 1, permission 1");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    for dispatch in [false, true] {
        let error = Prepared::new_with_dispatch(&bad, dispatch)
            .unwrap()
            .run(20000)
            .unwrap_err();
        assert!(error.starts_with("guest memory/SP fault at PC "));
    }
    // A guest store into the registered constant must fault in every engine.
    let mut words = Vec::new();
    for shift in 0..4 {
        words.push(
            (if shift == 0 { 0xd2800009 } else { 0xf2800009 })
                | shift << 21
                | (((cf >> (shift * 16)) & 65535) as u32) << 5,
        );
    }
    words.push(0xf900013f);
    let code = words
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    let mut bad = image.clone();
    bad.entry = 0x1000;
    bad.startup = None;
    bad.extra_text.push((0x1000, code.len() as u64));
    bad.segments.push(Segment {
        address: 0x1000,
        bytes: code.into(),
        protection: 5,
    });
    let mut cpu = sable::engine::Cpu::new(bad.entry, 200);
    let mut memory = sable::memory::Memory::new(&bad).unwrap();
    loop {
        let before = cpu.clone();
        match sable::engine::step_memory(&mut cpu, &bad, &mut memory) {
            Ok(false) => {}
            Ok(true) => panic!("unexpected trap before write fault"),
            Err(error) => {
                assert!(error.contains("permission 2"));
                assert_eq!(cpu, before);
                break;
            }
        }
    }
    assert_eq!(memory, sable::memory::Memory::new(&bad).unwrap());
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    for dispatch in [false, true] {
        let code = sable::jit::Compiled::new_with_dispatch(&bad, dispatch).unwrap();
        let mut other = sable::engine::Cpu::new(bad.entry, 200);
        let mut data = sable::memory::Memory::new(&bad).unwrap();
        assert!(
            code.execute_memory(&mut other, &bad, &mut data)
                .unwrap_err()
                .starts_with("guest memory/SP fault at PC ")
        );
        assert_eq!(other, cpu);
        assert_eq!(data, memory);
    }
    let selector = u64::from_le_bytes(
        image
            .read(addr(&raw, section(&raw, "__objc_selrefs")), 8, 1)
            .unwrap()
            .try_into()
            .unwrap(),
    );
    let send = image
        .code_ranges()
        .flat_map(|(a, n)| (a..a + n).step_by(4))
        .find(|&a| image.instruction(a).unwrap() == (0xd4000001 | (0x5ab3 << 5)))
        .unwrap()
        - 4;
    let mut words = Vec::new();
    for (reg, value) in [(0u32, 0x2000u64), (1, selector), (20, send)] {
        for shift in 0..4 {
            words.push(
                (if shift == 0 { 0xd2800000 } else { 0xf2800000 })
                    | reg
                    | shift << 21
                    | (((value >> (shift * 16)) & 65535) as u32) << 5,
            );
        }
    }
    words.push(0xd63f0280);
    let code = words
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    let mut forged = image.clone();
    forged.entry = 0x1000;
    forged.startup = None;
    forged.extra_text.push((0x1000, code.len() as u64));
    forged.segments.push(Segment {
        address: 0x1000,
        bytes: code.into(),
        protection: 5,
    });
    forged.segments.push(Segment {
        address: 0x2000,
        bytes: image.read(cf, 32, 1).unwrap().to_vec().into(),
        protection: 1,
    });
    assert_eq!(
        compare(&forged, 200).unwrap_err(),
        "unregistered constant-string object identity"
    );
}
