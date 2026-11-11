use sable::{
    engine::{Outcome, Prepared},
    macho::{Image, Segment},
    objc::{GET_CLASS, LOOKUP},
};
fn fixtures(mode: &str, variant: u8) -> (Vec<u8>, Vec<Vec<u8>>) {
    fixtures_profile("objc", mode, variant)
}
fn fixtures_profile(profile: &str, mode: &str, variant: u8) -> (Vec<u8>, Vec<Vec<u8>>) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/bin")
        .join(profile)
        .join(mode);
    (
        std::fs::read(root.join(format!("objc-{variant}.macho"))).unwrap(),
        [
            "libsable_other.dylib",
            "libsable_objc.dylib",
            "libsable_system.dylib",
        ]
        .iter()
        .map(|n| std::fs::read(root.join(n)).unwrap())
        .collect(),
    )
}
fn linked(mode: &str, variant: u8) -> Image {
    let (main, libs) = fixtures(mode, variant);
    Image::link(&main, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap()
}
fn compare(image: &Image, budget: u64) -> sable::Result<Outcome> {
    let outcome = Prepared::new(image, false).and_then(|p| p.run(budget));
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    for dispatch in [false, true] {
        let native = Prepared::new_with_dispatch(image, dispatch).and_then(|p| p.run(budget));
        match (&outcome, &native) {
            (Ok(a), Ok(b)) => assert_eq!(a, b),
            (Err(a), Err(b)) => assert_eq!(a, b),
            _ => panic!("ObjC engine mismatch: {outcome:?}/{native:?}"),
        }
    }
    outcome
}
// Native guards intentionally have different diagnostic text. For guard
// failures compare the rejection and full fault state, not presentation strings.
fn compare_guard_fault(image: &Image, budget: u64) -> String {
    use sable::{
        engine::{Cpu, step_memory},
        memory::Memory,
    };
    Prepared::new(image, false).unwrap();
    let mut cpu = Cpu::new(image.entry, budget);
    let mut memory = Memory::new(image).unwrap();
    let error = loop {
        match step_memory(&mut cpu, image, &mut memory) {
            Ok(false) => {}
            Ok(true) => panic!("guard probe reached unexpected service"),
            Err(error) => break error,
        }
    };
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    for dispatch in [false, true] {
        let code = sable::jit::Compiled::new_with_dispatch(image, dispatch).unwrap();
        let mut native_cpu = Cpu::new(image.entry, budget);
        let mut native_memory = Memory::new(image).unwrap();
        assert!(
            code.execute_memory(&mut native_cpu, image, &mut native_memory)
                .is_err()
        );
        assert_eq!(cpu, native_cpu);
        assert_eq!(memory, native_memory);
    }
    error
}
#[test]
fn compiled_methods_nested_messages_cross_image_selectors_abi_ctor_and_reset() {
    for profile in ["objc", "objc-compact"] {
        for mode in ["chained", "legacy"] {
            for variant in [0, 1] {
                let (main, libs) = fixtures_profile(profile, mode, variant);
                let image = Image::link(&main, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>())
                    .unwrap();
                if profile == "objc-compact" {
                    assert_compact(&image, &main);
                }
                assert!(image.objc.is_some());
                assert_eq!(image.startup.is_some(), variant == 1);
                let first = compare(&image, 10000).unwrap();
                assert_eq!(first.exit, 0);
                assert_eq!(
                    first.stdout,
                    b"ObjC root class, scalar instance/class dispatch, nil and selectors OK\n"
                );
                assert_eq!(first.syscalls, 2);
                if profile == "objc-compact" {
                    let (base, len, _) = section(&main, "__objc_methlist");
                    assert_eq!(
                        first.memory.read(base, len).unwrap(),
                        image.read(base, len, 1).unwrap()
                    );
                }
                let prepared = Prepared::new(&image, false).unwrap();
                for _ in 0..3 {
                    assert_eq!(prepared.run(10000).unwrap(), first);
                }
                #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                for dispatch in [false, true] {
                    let prepared = Prepared::new_with_dispatch(&image, dispatch).unwrap();
                    for _ in 0..3 {
                        assert_eq!(prepared.run(10000).unwrap(), first);
                    }
                }
                let (main, mut libs) = fixtures_profile(profile, mode, variant);
                libs.reverse();
                let reordered =
                    Image::link(&main, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>())
                        .unwrap();
                assert_eq!(compare(&reordered, 10000).unwrap(), first);
            }
        }
    }
}
#[test]
fn unknown_nonnil_selector_fails_explicitly() {
    for profile in ["objc", "objc-compact"] {
        for mode in ["chained", "legacy"] {
            let (main, libs) = fixtures_profile(profile, mode, 2);
            let image =
                Image::link(&main, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
            assert_eq!(
                compare(&image, 10000).unwrap_err(),
                "unknown ObjC selector for receiver"
            );
        }
    }
}
fn write(image: &mut Image, address: u64, data: &[u8]) {
    for s in &mut image.segments {
        if let Some(offset) = address
            .checked_sub(s.address)
            .and_then(|v| usize::try_from(v).ok())
            && offset
                .checked_add(data.len())
                .is_some_and(|end| end <= s.bytes.len())
        {
            std::sync::Arc::make_mut(&mut s.bytes)[offset..offset + data.len()]
                .copy_from_slice(data);
            return;
        }
    }
    panic!("test patch out of range");
}
fn section(raw: &[u8], name: &str) -> (u64, usize, usize) {
    let u32at = |at: usize| u32::from_le_bytes(raw[at..at + 4].try_into().unwrap()) as usize;
    let u64at = |at: usize| u64::from_le_bytes(raw[at..at + 8].try_into().unwrap());
    let mut at = 32;
    for _ in 0..u32at(16) {
        if u32at(at) == 0x19 {
            for n in 0..u32at(at + 64) {
                let s = at + 72 + n * 80;
                if &raw[s..s + name.len()] == name.as_bytes()
                    && (name.len() == 16 || raw[s + name.len()] == 0)
                {
                    return (u64at(s + 32), u64at(s + 40) as usize, s);
                }
            }
        }
        at += u32at(at + 4);
    }
    panic!("missing test section: {name}");
}
fn word(image: &Image, address: u64) -> u64 {
    u64::from_le_bytes(image.read(address, 8, 1).unwrap().try_into().unwrap())
}
fn selector(image: &Image, raw: &[u8], name: &str) -> u64 {
    let (base, len, _) = section(raw, "__objc_selrefs");
    for offset in (0..len).step_by(8) {
        let target = word(image, base + offset as u64);
        if image.read(target, name.len() + 1, 1).unwrap() == [name.as_bytes(), &[0]].concat() {
            return target;
        }
    }
    panic!("missing selector");
}
fn load(words: &mut Vec<u32>, reg: u32, value: u64) {
    for shift in 0..4 {
        words.push(
            (if shift == 0 { 0xd2800000 } else { 0xf2800000 })
                | shift << 21
                | (((value >> (shift * 16)) & 65535) as u32) << 5
                | reg,
        );
    }
}
fn synthetic(mut image: Image, words: &[u32], data: Vec<u8>) -> Image {
    image.entry = 0x1000;
    image.startup = None;
    let bytes = words
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect::<Vec<_>>();
    image.extra_text.push((0x1000, bytes.len() as u64));
    image.segments.push(Segment {
        address: 0x1000,
        bytes: bytes.into(),
        protection: 5,
    });
    image.segments.push(Segment {
        address: 0x3000,
        bytes: data.into(),
        protection: 3,
    });
    image
}
fn finish(words: &mut Vec<u32>) {
    words.extend([0xd2800000, 0xd2800030, 0xd4001001]);
}
fn trap_address(image: &Image, word: u32) -> u64 {
    for (base, len) in image.code_ranges() {
        for at in (base..base + len).step_by(4) {
            if image.instruction(at).unwrap() == word {
                return at;
            }
        }
    }
    panic!("missing runtime trap");
}
#[test]
fn exact_budget_lookup_tail_branch_imp_ret_and_nil() {
    for profile in ["objc", "objc-compact"] {
        let (raw, libs) = fixtures_profile(profile, "chained", 0);
        let image = Image::link(&raw, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
        let class = word(&image, section(&raw, "__objc_classlist").0);
        let sel = selector(&image, &raw, "classValue:");
        let runtime = trap_address(&image, LOOKUP) - 4;
        let mut words = Vec::new();
        load(&mut words, 0, class);
        load(&mut words, 1, sel);
        load(&mut words, 17, runtime);
        words.extend([0xd28004a2, 0xd63f0220]); // value37; BLR x17, returns through the guest IMP.
        finish(&mut words);
        let test = synthetic(image.clone(), &words, vec![0; 8]);
        let count = 1000 - compare(&test, 1000).unwrap().cpu.remaining;
        // Includes the final successful exit; check every edge, especially the private trap and RET.
        for budget in 0..=count + 1 {
            let outcome = compare(&test, budget);
            if budget < count {
                assert!(outcome.unwrap_err().contains("budget"));
            } else {
                assert_eq!(outcome.unwrap().exit, 0);
            }
        }
        let mut words = Vec::new();
        load(&mut words, 0, 0);
        load(&mut words, 1, u64::MAX);
        load(&mut words, 17, runtime);
        words.extend([0xd63f0220, 0xaa0003e9]);
        finish(&mut words);
        assert_eq!(
            compare(&synthetic(image, &words, vec![0; 8]), 100)
                .unwrap()
                .cpu
                .x[9],
            0
        );
    }
}
#[test]
fn foreign_traps_bad_receiver_selector_and_mutated_runtime_metadata_reject() {
    let image = linked("chained", 0);
    let (raw, _) = fixtures("chained", 0);
    let class = word(&image, section(&raw, "__objc_classlist").0);
    let meta = word(&image, class);
    let sel = selector(&image, &raw, "classValue:");
    let runtime = trap_address(&image, LOOKUP) - 4;
    for (receiver, selector, data) in [
        (0x3001, sel, vec![0; 8]),
        (meta, sel, vec![0; 8]),
        (0x3000, sel, vec![0; 8]),
        (class, sel + 1, vec![0; 8]),
        (class, 0, vec![0; 8]),
        (0x3000, sel, (class | 1).to_le_bytes().to_vec()),
    ] {
        let mut words = Vec::new();
        load(&mut words, 0, receiver);
        load(&mut words, 1, selector);
        load(&mut words, 17, runtime);
        words.push(0xd63f0220);
        finish(&mut words);
        assert!(compare(&synthetic(image.clone(), &words, data), 1000).is_err());
    }
    for svc in [GET_CLASS, LOOKUP] {
        assert!(
            compare(&synthetic(image.clone(), &[svc], vec![0; 8]), 10)
                .unwrap_err()
                .contains("unregistered ObjC trap")
        );
        let mut no_plan = synthetic(image.clone(), &[svc], vec![0; 8]);
        no_plan.objc = None;
        assert_eq!(compare(&no_plan, 10).unwrap_err(), "unregistered ObjC trap");
    }
    // The guest attempts to change a registered class field before entering the runtime.
    let mut words = Vec::new();
    load(&mut words, 9, class);
    load(&mut words, 10, 0);
    words.push(0xf900012a);
    load(&mut words, 0, class);
    load(&mut words, 1, sel);
    load(&mut words, 17, runtime);
    words.push(0xd63f0220);
    finish(&mut words);
    assert_eq!(
        compare(&synthetic(image.clone(), &words, vec![0; 8]), 1000).unwrap_err(),
        "mutated ObjC metadata unsupported"
    );
    for (address, bytes) in [
        (class, vec![0; 8]),
        (sel, vec![b'x']),
        (runtime + 4, vec![0; 4]),
    ] {
        let mut bad = image.clone();
        write(&mut bad, address, &bytes);
        assert!(Prepared::new(&bad, false).is_err());
    }
}
#[test]
fn malformed_sections_flags_metadata_and_missing_dependencies_reject() {
    for mode in ["chained", "legacy"] {
        let (main, libs) = fixtures(mode, 0);
        assert!(Image::parse(&main).is_err());
        assert!(
            Image::link(&main, &[&libs[2]])
                .unwrap_err()
                .contains("required dylib")
        );
        for (name, field, bytes) in [
            ("__objc_classlist", 40, 7u64.to_le_bytes().to_vec()),
            ("__objc_classlist", 32, u64::MAX.to_le_bytes().to_vec()),
            ("__objc_selrefs", 52, 32u32.to_le_bytes().to_vec()),
            ("__objc_const", 48, 1u32.to_le_bytes().to_vec()),
            ("__objc_imageinfo", 64, 1u32.to_le_bytes().to_vec()),
        ] {
            let mut bad = main.clone();
            let (_, _, at) = section(&bad, name);
            bad[at + field..at + field + bytes.len()].copy_from_slice(&bytes);
            assert!(
                Image::link(&bad, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).is_err()
            );
        }
        let (_, _, at) = section(&main, "__objc_imageinfo");
        let file_offset = u32::from_le_bytes(main[at + 48..at + 52].try_into().unwrap()) as usize;
        for (offset, value) in [(0, 1u32), (4, 0), (4, 0x41), (4, 0x80)] {
            let mut bad = main.clone();
            bad[file_offset + offset..file_offset + offset + 4]
                .copy_from_slice(&value.to_le_bytes());
            assert!(
                Image::link(&bad, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>())
                    .unwrap_err()
                    .contains("ObjC image flags")
            );
        }
        let mut bad = main.clone();
        let (_, _, at) = section(&bad, "__objc_classlist");
        bad[at..at + 16].copy_from_slice(b"__objc_catlist\0\0");
        assert!(
            Image::link(&bad, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>())
                .unwrap_err()
                .contains("unsupported ObjC section")
        );
    }
}

fn assert_compact(image: &Image, raw: &[u8]) {
    let (base, len, s) = section(raw, "__objc_methlist");
    let offset = u32::from_le_bytes(raw[s + 48..s + 52].try_into().unwrap()) as usize;
    assert_eq!(
        image.read(base, len, 1).unwrap(),
        &raw[offset..offset + len]
    );
    for at in (base..base + len as u64).step_by(4) {
        assert!(!image.contains_pc(at));
    }
    let class = word(image, section(raw, "__objc_classlist").0);
    let mut four_aligned = false;
    for object in [class, word(image, class)] {
        let ro = word(image, object + 32);
        let methods = word(image, ro + 32);
        assert!(methods >= base && methods + 8 <= base + len as u64);
        assert_eq!(
            image.read(methods, 4, 1).unwrap(),
            0x8000000cu32.to_le_bytes()
        );
        four_aligned |= methods % 8 == 4;
    }
    assert!(
        four_aligned,
        "fixture must exercise a 4-byte-only-aligned list"
    );
}
#[test]
fn mixed_formats_and_compact_metadata_are_immutable_noncode() {
    for mode in ["chained", "legacy"] {
        for (main_profile, lib_profile) in [("objc", "objc-compact"), ("objc-compact", "objc")] {
            let (main, _) = fixtures_profile(main_profile, mode, 1);
            let (_, libs) = fixtures_profile(lib_profile, mode, 1);
            let image =
                Image::link(&main, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
            assert_eq!(compare(&image, 10000).unwrap().exit, 0);
        }
    }
    let (raw, libs) = fixtures_profile("objc-compact", "chained", 0);
    let image = Image::link(&raw, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
    let (table, _, _) = section(&raw, "__objc_methlist");
    let (slots, _, _) = section(&raw, "__objc_selrefs");
    for address in [table, table + 8, slots] {
        let mut modified = image.clone();
        write(&mut modified, address, &[0; 4]);
        assert!(Prepared::new(&modified, false).is_err());
    }
    for (address, message) in [
        (table, "permission 2"),
        (slots, "mutated ObjC metadata unsupported"),
    ] {
        let mut words = Vec::new();
        load(&mut words, 9, address);
        load(&mut words, 10, 0);
        words.push(0xf900012a); // STR x10,[x9]. RX compact entries must fault before mutation.
        load(&mut words, 0, slots);
        load(&mut words, 17, trap_address(&image, GET_CLASS));
        words.push(0xd63f0220);
        let probe = synthetic(image.clone(), &words, vec![0; 8]);
        let error = if address == table {
            compare_guard_fault(&probe, 100)
        } else {
            compare(&probe, 100).unwrap_err()
        };
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn compact_section_shape_code_flags_and_mixed_malformed_list_reject() {
    for mode in ["chained", "legacy"] {
        let (raw, libs) = fixtures_profile("objc-compact", mode, 0);
        for (field, bytes) in [
            (32, 1u64.to_le_bytes().to_vec()),
            (40, 3u64.to_le_bytes().to_vec()),
            (48, 1u32.to_le_bytes().to_vec()),
            (52, 32u32.to_le_bytes().to_vec()),
            (64, 0x90000400u32.to_le_bytes().to_vec()),
        ] {
            let mut modified = raw.clone();
            let (_, _, s) = section(&modified, "__objc_methlist");
            modified[s + field..s + field + bytes.len()].copy_from_slice(&bytes);
            assert!(
                Image::link(
                    &modified,
                    &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()
                )
                .is_err()
            );
        }
        let mut modified = raw.clone();
        let (_, _, s) = section(&modified, "__objc_methlist");
        let off = u32::from_le_bytes(modified[s + 48..s + 52].try_into().unwrap()) as usize;
        modified[off..off + 4].copy_from_slice(&0xc000000cu32.to_le_bytes());
        let (_, absolute) = fixtures_profile("objc", mode, 0);
        assert!(
            Image::link(
                &modified,
                &absolute.iter().map(Vec::as_slice).collect::<Vec<_>>()
            )
            .unwrap_err()
            .contains("method list flags")
        );
        let image = Image::link(&raw, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
        let mut words = Vec::new();
        load(&mut words, 17, section(&raw, "__objc_methlist").0);
        words.push(0xd61f0220);
        assert!(
            compare_guard_fault(&synthetic(image, &words, vec![0; 8]), 100)
                .contains("PC outside __text")
        );
    }
}
