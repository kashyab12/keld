use sable::{
    engine::{Outcome, Prepared},
    macho::{Image, Segment},
    memory::{STACK_BASE, STACK_SIZE},
};
const GET: u32 = 0xd4000001 | (0x5ab4 << 5);
const NAME: u32 = 0xd4000001 | (0x5ab5 << 5);
const CONFORMS: u32 = 0xd4000001 | (0x5ab6 << 5);
fn fixtures(format: &str, mode: &str, ctor: u8) -> (Vec<u8>, Vec<Vec<u8>>) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/bin/protocols")
        .join(format)
        .join(mode);
    (
        std::fs::read(root.join(format!("protocol-{ctor}.macho"))).unwrap(),
        ["libsable_objc.dylib", "libsable_system.dylib"]
            .iter()
            .map(|n| std::fs::read(root.join(n)).unwrap())
            .collect(),
    )
}
fn linked() -> Image {
    let (raw, libs) = fixtures("absolute", "chained", 0);
    Image::link(&raw, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap()
}
fn compare(image: &Image, budget: u64) -> sable::Result<Outcome> {
    let result = Prepared::new(image, false).and_then(|p| p.run(budget));
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    for dispatch in [false, true] {
        let native = Prepared::new_with_dispatch(image, dispatch).and_then(|p| p.run(budget));
        match (&result, &native) {
            (Ok(a), Ok(b)) => assert_eq!(a, b),
            (Err(a), Err(b)) => assert_eq!(a, b),
            _ => panic!("protocol mismatch: {result:?}/{native:?}"),
        }
    }
    result
}
#[test]
fn compiled_protocol_identity_adoption_real_method_abi_ctor_and_fresh_data() {
    for format in ["absolute", "compact"] {
        for mode in ["chained", "legacy"] {
            for ctor in [0, 1] {
                let (raw, mut libs) = fixtures(format, mode, ctor);
                let image =
                    Image::link(&raw, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
                let first = compare(&image, 20000).unwrap();
                assert_eq!(first.exit, 0);
                assert_eq!(first.syscalls, 2);
                assert_eq!(
                    first.stdout,
                    b"Protocol lookup, names, declared adoption and guest IMP OK\n"
                );
                let prepared = Prepared::new(&image, false).unwrap();
                for _ in 0..3 {
                    assert_eq!(prepared.run(20000).unwrap(), first);
                }
                #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                for dispatch in [false, true] {
                    let prepared = Prepared::new_with_dispatch(&image, dispatch).unwrap();
                    for _ in 0..3 {
                        assert_eq!(prepared.run(20000).unwrap(), first);
                    }
                }
                libs.reverse();
                let reordered =
                    Image::link(&raw, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
                assert_eq!(compare(&reordered, 20000).unwrap(), first);
            }
        }
    }
}
fn section(raw: &[u8], name: &str) -> (u64, usize) {
    let u32at = |a: usize| u32::from_le_bytes(raw[a..a + 4].try_into().unwrap()) as usize;
    let mut at = 32;
    for _ in 0..u32at(16) {
        if u32at(at) == 0x19 {
            for i in 0..u32at(at + 64) {
                let s = at + 72 + i * 80;
                if &raw[s..s + name.len()] == name.as_bytes()
                    && (name.len() == 16 || raw[s + name.len()] == 0)
                {
                    return (
                        u64::from_le_bytes(raw[s + 32..s + 40].try_into().unwrap()),
                        s,
                    );
                }
            }
        }
        at += u32at(at + 4);
    }
    panic!("missing section {name}");
}
fn identities(image: &Image, raw: &[u8]) -> (u64, u64) {
    let find = |section_name: &str, name: &str, is_class: bool| {
        let (base, header) = section(raw, section_name);
        let len = u64::from_le_bytes(raw[header + 40..header + 48].try_into().unwrap());
        for offset in (0..len).step_by(8) {
            let address = word(image, base + offset);
            let name_address = if is_class {
                word(image, word(image, address + 32) + 24)
            } else {
                word(image, address + 8)
            };
            if image.read(name_address, name.len() + 1, 1).unwrap()
                == [name.as_bytes(), &[0]].concat()
            {
                return address;
            }
        }
        panic!("missing metadata identity {name}");
    };
    (
        find("__objc_protolist", "SableScalar", false),
        find("__objc_classlist", "ProtocolRoot", true),
    )
}
fn word(image: &Image, address: u64) -> u64 {
    u64::from_le_bytes(image.read(address, 8, 1).unwrap().try_into().unwrap())
}
fn find_trap(image: &Image, word: u32) -> u64 {
    for (base, len) in image.code_ranges() {
        for at in (base..base + len).step_by(4) {
            if image.instruction(at).unwrap() == word {
                return at;
            }
        }
    }
    panic!("missing service");
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
fn synthetic(mut image: Image, words: Vec<u32>, data: Vec<u8>) -> Image {
    let code = words
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    image.entry = 0x1000;
    image.startup = None;
    image.extra_text.push((0x1000, code.len() as u64));
    image.segments.push(Segment {
        address: 0x1000,
        bytes: code.into(),
        protection: 5,
    });
    image.segments.push(Segment {
        address: 0x3000,
        bytes: data.into(),
        protection: 3,
    });
    image
}
fn query(image: Image, service: u32, a: u64, b: u64) -> (Image, [u64; 31], u64) {
    let function = find_trap(&image, service);
    let mut words = vec![0xd10803ff]; // SUB SP,SP,#512.
    let mut expected = [0u64; 31];
    for (i, value) in expected.iter_mut().enumerate() {
        *value = 0x1234000000000000 + i as u64;
    }
    expected[0] = a;
    expected[1] = b;
    expected[17] = function;
    for (i, &value) in expected.iter().enumerate() {
        load(&mut words, i as u32, value);
    }
    words.push(0xeb00001f); // CMP X0,X0, Z/C set. New service must leave NZCV unchanged.
    let resume = 0x1000 + (words.len() as u64 + 1) * 4;
    expected[30] = resume;
    words.push(0xd63f0220);
    for i in 0..31 {
        words.push(0xf9000000 | i << 10 | 31 << 5 | i);
    }
    words.extend([0x910003e0, 0xf9007fe0, 0xd2800000, 0xd2800030, 0xd4001001]);
    (
        synthetic(image, words, b"MissingProtocol\0".to_vec()),
        expected,
        STACK_BASE + STACK_SIZE as u64 - 512,
    )
}
#[test]
fn services_preserve_modeled_registers_flags_and_every_budget_edge() {
    let image = linked();
    let (raw, _) = fixtures("absolute", "chained", 0);
    let (protocol, class) = identities(&image, &raw);
    let name = word(&image, protocol + 8);
    for (service, a, b, expected_result) in [
        (GET, name, 0, protocol),
        (NAME, protocol, 0, name),
        (CONFORMS, class, protocol, 1),
    ] {
        let (probe, expected, sp) = query(image.clone(), service, a, b);
        let prepared = Prepared::new(&probe, false).unwrap();
        let first = prepared.run(1000).unwrap();
        let count = 1000 - first.cpu.remaining;
        for i in 0..31 {
            let actual = u64::from_le_bytes(
                first
                    .memory
                    .read(sp + i * 8, 8)
                    .unwrap()
                    .try_into()
                    .unwrap(),
            );
            assert_eq!(
                actual,
                if i == 0 {
                    expected_result
                } else {
                    expected[i as usize]
                },
                "service={service:#x},reg={i}"
            );
        }
        assert_eq!(word_memory(&first, sp + 248), sp);
        assert_eq!(first.cpu.nzcv, 0x60000000);
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let native = [
            Prepared::new_with_dispatch(&probe, false).unwrap(),
            Prepared::new_with_dispatch(&probe, true).unwrap(),
        ];
        for budget in 0..=count + 1 {
            let outcome = prepared.run(budget);
            if budget < count {
                assert!(outcome.as_ref().unwrap_err().contains("budget"));
            } else {
                assert_eq!(outcome.as_ref().unwrap().exit, 0);
            }
            #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
            for code in &native {
                match (&outcome, code.run(budget)) {
                    (Ok(a), Ok(b)) => assert_eq!(a, &b),
                    (Err(a), Err(b)) => assert_eq!(a, &b),
                    (a, b) => panic!("budget mismatch {a:?}/{b:?}"),
                }
            }
        }
    }
}
fn word_memory(out: &Outcome, address: u64) -> u64 {
    u64::from_le_bytes(out.memory.read(address, 8).unwrap().try_into().unwrap())
}
#[test]
fn null_missing_and_unknown_identities_and_foreign_services() {
    let image = linked();
    let (raw, _) = fixtures("absolute", "chained", 0);
    let (p, class) = identities(&image, &raw);
    for (svc, a, b) in [(GET, 0x3000, 0), (CONFORMS, 0, p), (CONFORMS, class, 0)] {
        let (probe, _, sp) = query(image.clone(), svc, a, b);
        assert_eq!(word_memory(&compare(&probe, 1000).unwrap(), sp), 0);
    }
    let (probe, _, sp) = query(image.clone(), NAME, 0, 0);
    let out = compare(&probe, 1000).unwrap();
    assert_eq!(out.memory.read(word_memory(&out, sp), 4).unwrap(), b"nil\0");
    for (svc, a, b) in [
        (GET, u64::MAX, 0),
        (NAME, p + 8, 0),
        (NAME, class, 0),
        (CONFORMS, u64::MAX, p),
        (CONFORMS, class, p + 8),
    ] {
        let (probe, _, _) = query(image.clone(), svc, a, b);
        assert!(compare(&probe, 1000).is_err());
    }
    let selector = word(&image, section(&raw, "__objc_selrefs").0);
    let (opaque, _, _) = query(image.clone(), 0xd4000001 | (0x5ab3 << 5), p, selector);
    assert_eq!(
        compare(&opaque, 1000).unwrap_err(),
        "unregistered ObjC raw isa"
    );
    for svc in [GET, NAME, CONFORMS] {
        let probe = synthetic(image.clone(), vec![svc], vec![0; 16]);
        assert_eq!(compare(&probe, 100).unwrap_err(), "unregistered ObjC trap");
    }
}
fn overwrite(image: &mut Image, address: u64, bytes: &[u8]) {
    for s in &mut image.segments {
        if let Some(at) = address
            .checked_sub(s.address)
            .and_then(|n| usize::try_from(n).ok())
            && at
                .checked_add(bytes.len())
                .is_some_and(|end| end <= s.bytes.len())
        {
            std::sync::Arc::make_mut(&mut s.bytes)[at..at + bytes.len()].copy_from_slice(bytes);
            return;
        }
    }
    panic!("patch outside image");
}
#[test]
fn prepared_and_live_protocol_record_ref_and_adoption_mutation_reject() {
    let image = linked();
    let (raw, _) = fixtures("absolute", "chained", 0);
    let (p, class) = identities(&image, &raw);
    let name = word(&image, p + 8);
    let ro = word(&image, class + 32);
    let adoption = word(&image, ro + 40);
    let descriptions = word(&image, p + 24);
    for address in [
        p,
        p + 8,
        p + 64,
        name,
        adoption + 8,
        descriptions + 8,
        section(&raw, "__objc_protorefs").0,
    ] {
        let mut bad = image.clone();
        overwrite(&mut bad, address, &[1; 8]);
        assert!(Prepared::new(&bad, false).is_err());
    }
    for (address, message) in [
        (p, "mutated protocol body unsupported"),
        (adoption + 8, "mutated ObjC metadata unsupported"),
        (
            section(&raw, "__objc_protorefs").0,
            "mutated ObjC metadata unsupported",
        ),
    ] {
        let mut words = Vec::new();
        load(&mut words, 9, address);
        load(&mut words, 10, 1);
        words.push(0xf900012a);
        load(&mut words, 0, p);
        load(&mut words, 17, find_trap(&image, NAME));
        words.push(0xd63f0220);
        assert_eq!(
            compare(&synthetic(image.clone(), words, vec![0; 8]), 100).unwrap_err(),
            message
        );
    }
    let mut alias = image.clone();
    alias.extra_text.push((p, 96));
    assert!(
        Prepared::new(&alias, false)
            .err()
            .unwrap()
            .contains("metadata overlaps")
    );
}

#[test]
fn test_bit_branch_decoding_covers_extreme_signed_displacements() {
    use sable::isa::{Op, decode};
    for bit in [0u32, 31, 32, 63] {
        for nonzero in [false, true] {
            for reg in [0u32, 31] {
                for displacement in [-8192i32, -1, 0, 1, 8191] {
                    let word = 0x36000000
                        | (bit >> 5) << 31
                        | u32::from(nonzero) << 24
                        | (bit & 31) << 19
                        | ((displacement as u32) & 0x3fff) << 5
                        | reg;
                    assert_eq!(
                        decode(word, 0x10000).unwrap(),
                        Op::TestBranch {
                            reg: reg as u8,
                            bit: bit as u8,
                            nonzero,
                            target: 0x10000u64.wrapping_add_signed(i64::from(displacement) * 4)
                        }
                    );
                }
            }
        }
    }
}

#[test]
fn unsigned_bitfield_rejects_reserved_widths_and_other_bitfield_operations() {
    use sable::isa::{Op, decode};
    assert_eq!(
        decode(0x531e7500, 0x1000).unwrap(),
        Op::UnsignedBitfield {
            dst: 0,
            src: 8,
            right: 0,
            left: 2,
            mask: (1 << 30) - 1,
            wide: false,
        }
    );
    for word in [
        0x53400000, 0xd3000000, 0x53200000, 0x53008000, 0x13000000, 0x33000000,
    ] {
        assert!(decode(word, 0x1000).is_err(), "accepted {word:#x}");
    }
}
