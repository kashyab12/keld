use sable::{
    engine::Prepared,
    macho::{Image, Segment},
    tls::{BASE, Block, Descriptor, LIMIT, SVC},
};

fn fixtures(mode: &str, ctor: u8) -> (Vec<u8>, Vec<Vec<u8>>) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/bin/tls")
        .join(mode);
    (
        std::fs::read(root.join(format!("tls-{ctor}.macho"))).unwrap(),
        ["libtlv_a.dylib", "libtlv_b.dylib", "libsable_system.dylib"]
            .iter()
            .map(|n| std::fs::read(root.join(n)).unwrap())
            .collect(),
    )
}
#[test]
fn two_images_both_fixups_scalar_abi_constructors_and_private_prepared_reset() {
    for mode in ["chained", "legacy"] {
        for ctor in [0, 1] {
            let (main, mut libs) = fixtures(mode, ctor);
            let image =
                Image::link(&main, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
            assert_eq!(image.tls.len(), 2);
            assert_eq!(image.startup.is_some(), ctor == 1);
            let prepared = Prepared::new(&image, false).unwrap();
            let first = prepared.run(10000).unwrap();
            assert_eq!(first.exit, 0);
            assert_eq!(
                first.stdout,
                b"TLV scalar ABI, templates, two images, stable addresses OK\n"
            );
            assert_eq!(first.syscalls, 2); // The private resolver is not a Darwin syscall.
            for _ in 0..3 {
                assert_eq!(prepared.run(10000).unwrap(), first);
            }
            for (offset, values) in [(0, [23u64, 7]), (16, [41u64, 9])] {
                assert_eq!(
                    first.memory.read(BASE + offset, 16).unwrap(),
                    values
                        .iter()
                        .flat_map(|n| n.to_le_bytes())
                        .collect::<Vec<_>>()
                );
            }
            for block in &image.tls {
                assert_eq!(
                    image.read(block.source, block.template.len(), 3).unwrap(),
                    block.template.as_slice()
                );
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
            for dispatch in [false, true] {
                let native = Prepared::new_with_dispatch(&image, dispatch).unwrap();
                for _ in 0..3 {
                    assert_eq!(native.run(10000).unwrap(), first);
                }
            }
        }
    }
}

fn synthetic(prefix: &[u32], descriptor: u64, foreign_trap: bool) -> Image {
    let mut words = vec![SVC, 0xd65f03c0];
    words.extend(prefix);
    words.push(0xd2800000 | ((descriptor as u32) << 5)); // MOVZ x0, descriptor.
    let call = 0x1000 + words.len() as u64 * 4;
    words.push(if foreign_trap {
        SVC
    } else {
        0x94000000 | ((((0x1000i64 - call as i64) / 4) as u32) & 0x3ffffff)
    });
    words.extend([0xaa0003e9, 0xd2800000, 0xd2800030, 0xd4001001]);
    let code: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    let mut data = vec![0; 64];
    data[..8].copy_from_slice(&0x1000u64.to_le_bytes());
    data[24..32].copy_from_slice(&17u64.to_le_bytes());
    Image {
        segments: vec![
            Segment {
                address: 0x1000,
                bytes: code.clone().into(),
                protection: 5,
            },
            Segment {
                address: 0x2000,
                bytes: data.into(),
                protection: 3,
            },
        ],
        entry: 0x1008,
        text: (0x1000, code.len() as u64),
        extra_text: vec![],
        platform: 7,
        startup: None,
        objc: None,
        tls: vec![Block {
            source: 0x2018,
            template: 17u64.to_le_bytes().to_vec().into(),
            alignment: 8,
            descriptors: vec![Descriptor {
                address: 0x2000,
                resolver: 0x1000,
                offset: 0,
            }],
        }],
    }
}
fn compare_engines(image: &Image, budget: u64) -> sable::Result<sable::engine::Outcome> {
    let result = Prepared::new(image, false).and_then(|p| p.run(budget));
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    for dispatch in [false, true] {
        let native = Prepared::new_with_dispatch(image, dispatch).and_then(|p| p.run(budget));
        match (&result, &native) {
            (Ok(a), Ok(b)) => assert_eq!(a, b),
            (Err(a), Err(b)) => assert_eq!(a, b),
            _ => panic!("different TLS outcomes: {result:?}/{native:?}"),
        }
    }
    result
}
#[test]
fn resolver_call_svc_ret_budget_edges_and_nested_return() {
    let image = synthetic(&[], 0x2000, false);
    for budget in 0..=10 {
        let outcome = compare_engines(&image, budget);
        if budget < 8 {
            assert!(outcome.unwrap_err().contains("budget"));
        } else {
            let outcome = outcome.unwrap();
            assert_eq!(outcome.exit, 0);
            assert_eq!(outcome.cpu.x[9], BASE);
        }
    }
}
#[test]
fn unknown_middle_mutated_descriptors_and_foreign_traps_reject() {
    for descriptor in [0x2008, 0x2020, 0x2040] {
        assert!(
            compare_engines(&synthetic(&[], descriptor, false), 100)
                .unwrap_err()
                .contains("unknown TLS descriptor")
        );
    }
    assert!(
        compare_engines(&synthetic(&[], 0x2000, true), 100)
            .unwrap_err()
            .contains("foreign resolver trap")
    );
    for field in [0, 8, 16] {
        let prefix = [0xd2840000, 0xd2800021, 0xf9000001 | ((field / 8) << 10)];
        assert!(
            compare_engines(&synthetic(&prefix, 0x2000, false), 100)
                .unwrap_err()
                .contains("mutated TLS descriptor")
        );
    }
    let mut no_registry = synthetic(&[], 0x2000, false);
    no_registry.tls.clear();
    assert!(
        compare_engines(&no_registry, 100)
            .unwrap_err()
            .contains("unknown TLS descriptor")
    );
}
#[test]
fn invalid_prepared_templates_offsets_caps_and_reserved_arena_reject() {
    let original = synthetic(&[], 0x2000, false);
    for offset in [8, u64::MAX] {
        let mut image = original.clone();
        image.tls[0].descriptors[0].offset = offset;
        assert!(Prepared::new(&image, false).is_err());
    }
    for alignment in [0, 3, 8192] {
        let mut image = original.clone();
        image.tls[0].alignment = alignment;
        assert!(Prepared::new(&image, false).is_err());
    }
    let mut image = original.clone();
    image.tls[0].template = vec![0; LIMIT + 1].into();
    assert!(Prepared::new(&image, false).is_err());
    let mut image = original.clone();
    image.tls.push(image.tls[0].clone());
    assert!(Prepared::new(&image, false).is_err());
    let mut image = original.clone();
    image.segments.push(Segment {
        address: BASE,
        bytes: vec![0; 16].into(),
        protection: 3,
    });
    assert!(Prepared::new(&image, false).is_err());
    let mut image = original;
    image.tls[0].descriptors[0].resolver = 0x1008;
    assert!(Prepared::new(&image, false).is_err());
}

#[test]
fn zero_only_template_and_large_alignment_use_private_backing() {
    let mut image = synthetic(&[], 0x2000, false);
    std::sync::Arc::make_mut(&mut image.segments[1].bytes)[24..32].fill(0);
    image.tls[0].template = vec![0; 8].into();
    image.tls[0].alignment = 4096;
    let first = compare_engines(&image, 100).unwrap();
    assert_eq!(first.memory.read(BASE, 8).unwrap(), [0; 8]);
    assert_eq!(first.cpu.x[9], BASE);
}

fn section(bytes: &[u8], kind: u32) -> usize {
    let word = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let mut at = 32;
    for _ in 0..word(16) {
        if word(at) == 0x19 {
            for i in 0..word(at + 64) as usize {
                let s = at + 72 + i * 80;
                if word(s + 64) & 255 == kind {
                    return s;
                }
            }
        }
        at += word(at + 4) as usize;
    }
    panic!("missing TLS section")
}
#[test]
fn malformed_disk_metadata_rejects_before_execution() {
    for mode in ["chained", "legacy"] {
        let (main, libs) = fixtures(mode, 0);
        let s = section(&libs[0], 0x13);
        for (field, value) in [(40, 23u64), (32, 1), (32, u64::MAX - 15)] {
            let mut bad = libs.clone();
            bad[0][s + field..s + field + 8].copy_from_slice(&value.to_le_bytes());
            assert!(
                Image::link(&main, &bad.iter().map(Vec::as_slice).collect::<Vec<_>>()).is_err()
            );
        }
        for (field, value) in [(52, 32u32), (64, 0x15), (48, 1)] {
            let mut bad = libs.clone();
            bad[0][s + field..s + field + 4].copy_from_slice(&value.to_le_bytes());
            assert!(
                Image::link(&main, &bad.iter().map(Vec::as_slice).collect::<Vec<_>>()).is_err()
            );
        }
        let offset = u32::from_le_bytes(libs[0][s + 48..s + 52].try_into().unwrap()) as usize;
        for (field, value) in [(8, 1u64), (16, u64::MAX)] {
            let mut bad = libs.clone();
            bad[0][offset + field..offset + field + 8].copy_from_slice(&value.to_le_bytes());
            assert!(
                Image::link(&main, &bad.iter().map(Vec::as_slice).collect::<Vec<_>>()).is_err()
            );
        }
        assert!(Image::parse(&main).is_err());
        assert!(Image::link(&main, &[&libs[0], &libs[1]]).is_err());
    }
}
