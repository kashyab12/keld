use sable::{
    engine::Prepared,
    macho::{Image, Segment, Startup},
    memory::{STACK_BASE, STACK_SIZE},
};

const EXPECTED: &[u8] =
    b"INIT leaf 1\nINIT leaf 2\nINIT parent\nINIT sibling\nINIT executable\nARGS\nMAIN\n";

fn fixtures(mode: &str) -> (Vec<u8>, Vec<Vec<u8>>) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/bin/initializers")
        .join(mode);
    (
        std::fs::read(path.join("initialize.macho")).unwrap(),
        ["parent", "sibling", "leaf", "system"]
            .iter()
            .map(|name| std::fs::read(path.join(format!("libsable_{name}.dylib"))).unwrap())
            .collect(),
    )
}

#[test]
fn dependency_array_order_diamond_once_arguments_and_private_reset() {
    for mode in ["offsets", "pointers"] {
        let (main, mut libs) = fixtures(mode);
        let image =
            Image::link(&main, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
        assert_eq!(image.startup.as_ref().unwrap().initializers.len(), 6);
        let prepared = Prepared::new(&image, false).unwrap();
        let first = prepared.run(10000).unwrap();
        assert_eq!(first.stdout, EXPECTED);
        assert_eq!(first.exit, 0);
        for _ in 0..3 {
            assert_eq!(prepared.run(10000).unwrap(), first);
        }
        libs.reverse();
        let reordered =
            Image::link(&main, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
        assert_eq!(
            Prepared::new(&reordered, false)
                .unwrap()
                .run(10000)
                .unwrap(),
            first
        );
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        for indirect in [false, true] {
            let jit = Prepared::new_with_dispatch(&image, indirect).unwrap();
            for _ in 0..3 {
                assert_eq!(jit.run(10000).unwrap(), first);
            }
        }
        // Default launch profile's arrays and ProgramVars live in this run's stack.
        let base = STACK_BASE + STACK_SIZE as u64 - 4096;
        assert_eq!(first.memory.read(base + 0x200, 6).unwrap(), b"sable\0");
        assert_eq!(first.memory.read(base + 0x120, 8).unwrap(), [0; 8]);
    }
}

fn synthetic(words: &[u32], entry: usize) -> Image {
    let mut bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    bytes.resize(bytes.len().max(32), 0);
    let len = bytes.len() as u64;
    Image {
        segments: vec![Segment {
            address: 0x1000,
            bytes: bytes.into(),
            protection: 5,
        }],
        entry: 0x1000 + entry as u64 * 4,
        text: (0x1000, len),
        extra_text: vec![],
        platform: 7,
        tls: Vec::new(),
        objc: None,
        startup: Some(Startup {
            header: 0x1000,
            initializers: vec![0x1000],
        }),
    }
}

#[test]
fn constructor_return_at_exact_budget_and_main_return_status() {
    let image = synthetic(&[0xd65f03c0, 0xd28000e0, 0xd65f03c0], 1);
    for budget in 0..=4 {
        let reference = Prepared::new(&image, false).unwrap().run(budget);
        if budget >= 3 {
            assert_eq!(reference.as_ref().unwrap().exit, 7);
        } else {
            let error = reference.as_ref().unwrap_err();
            assert!(error.contains("budget"));
            assert_eq!(error.starts_with("initializer"), budget == 0);
        }
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        for indirect in [false, true] {
            let actual = Prepared::new_with_dispatch(&image, indirect)
                .unwrap()
                .run(budget);
            match (&reference, &actual) {
                (Ok(a), Ok(b)) => assert_eq!(a, b),
                (Err(a), Err(b)) => {
                    assert_eq!(a.contains("budget"), b.contains("budget"));
                    assert_eq!(a.starts_with("initializer"), b.starts_with("initializer"));
                }
                _ => panic!("different outcomes at budget {budget}: {reference:?}/{actual:?}"),
            }
        }
    }
}

#[test]
fn callback_exit_nonreturn_and_unbalanced_stack_stop_startup() {
    for (image, status) in [
        (
            synthetic(
                &[0xd28002e0, 0xd2800030, 0xd4001001, 0xd2800000, 0xd65f03c0],
                3,
            ),
            Some(23),
        ),
        (synthetic(&[0x14000000, 0xd65f03c0], 1), None),
        (synthetic(&[0xd10043ff, 0xd65f03c0, 0xd65f03c0], 2), None),
    ] {
        let reference = Prepared::new(&image, false).unwrap().run(100);
        if let Some(code) = status {
            assert_eq!(reference.as_ref().unwrap().exit, code);
        } else {
            assert!(reference.is_err());
        }
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        for indirect in [false, true] {
            let actual = Prepared::new_with_dispatch(&image, indirect)
                .unwrap()
                .run(100);
            match (&reference, &actual) {
                (Ok(a), Ok(b)) => assert_eq!(a, b),
                (Err(a), Err(b)) => assert_eq!(a, b),
                _ => panic!("different startup outcomes: {reference:?}/{actual:?}"),
            }
        }
    }
}

fn section(bytes: &[u8]) -> usize {
    let mut at = 32;
    for _ in 0..u32::from_le_bytes(bytes[16..20].try_into().unwrap()) {
        if u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) == 0x19 {
            for i in 0..u32::from_le_bytes(bytes[at + 64..at + 68].try_into().unwrap()) as usize {
                let s = at + 72 + i * 80;
                if [9, 22].contains(
                    &(u32::from_le_bytes(bytes[s + 64..s + 68].try_into().unwrap()) & 255),
                ) {
                    return s;
                }
            }
        }
        at += u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
    }
    panic!("missing initializer array")
}

#[test]
fn malformed_arrays_and_targets_reject_before_execution() {
    for mode in ["offsets", "pointers"] {
        let (main, libs) = fixtures(mode);
        let s = section(&main);
        for (field, value) in [(40, 3u64), (32, 1u64), (32, u64::MAX - 3)] {
            let mut bad = main.clone();
            bad[s + field..s + field + 8].copy_from_slice(&value.to_le_bytes());
            assert!(
                Image::link(&bad, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).is_err()
            );
        }
        let mut bad = main.clone();
        let offset = u32::from_le_bytes(bad[s + 48..s + 52].try_into().unwrap()) as usize;
        // Offset zero targets the Mach-O header; pointer zero is not executable.
        bad[offset..offset + if mode == "offsets" { 4 } else { 8 }].fill(0);
        assert!(Image::link(&bad, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).is_err());
    }
}
