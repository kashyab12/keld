use sable::{
    engine::{Outcome, Prepared},
    macho::{Image, Segment},
    memory::{STACK_BASE, STACK_SIZE},
};
const SUPER: u32 = 0xd4000001 | (0x5ab7 << 5);
fn fixtures(fmt: &str, mode: &str, split: u8, ctor: u8) -> (Vec<u8>, Vec<Vec<u8>>) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/bin/super")
        .join(fmt)
        .join(mode)
        .join(split.to_string());
    let names = if split == 0 {
        vec!["libsable_objc.dylib", "libsable_system.dylib"]
    } else {
        vec![
            "libsable_base.dylib",
            "libsable_objc.dylib",
            "libsable_system.dylib",
        ]
    };
    (
        std::fs::read(root.join(format!("super-{ctor}.macho"))).unwrap(),
        names
            .iter()
            .map(|n| std::fs::read(root.join(n)).unwrap())
            .collect(),
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
            _ => panic!("Super2 mismatch {result:?}/{other:?}"),
        }
    }
    result
}
fn section(raw: &[u8], name: &str) -> (u64, usize) {
    let u32at = |n| u32::from_le_bytes(raw[n..n + 4].try_into().unwrap()) as usize;
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
fn word(i: &Image, a: u64) -> u64 {
    u64::from_le_bytes(i.read(a, 8, 1).unwrap().try_into().unwrap())
}
fn name(i: &Image, a: u64) -> String {
    (0..256)
        .map(|n| i.read(a + n, 1, 1).unwrap()[0])
        .take_while(|&c| c != 0)
        .map(char::from)
        .collect()
}
fn identities(i: &Image, raw: &[u8]) -> (u64, u64, u64, u64) {
    let (list, header) = section(raw, "__objc_classlist");
    let len = u64::from_le_bytes(raw[header + 40..header + 48].try_into().unwrap());
    let child = (0..len)
        .step_by(8)
        .map(|n| word(i, list + n))
        .find(|&p| name(i, word(i, word(i, p + 32) + 24)) == "SableChild")
        .unwrap();
    let base = word(i, child + 8);
    (child, word(i, child), base, word(i, base))
}
fn selector(i: &Image, class: u64, method: &str) -> u64 {
    let rows = word(i, word(i, class + 32) + 32);
    let header = i.read(rows, 8, 1).unwrap();
    assert_eq!(u32::from_le_bytes(header[..4].try_into().unwrap()), 24);
    let count = u32::from_le_bytes(header[4..].try_into().unwrap());
    (0..count)
        .map(|n| word(i, rows + 8 + u64::from(n) * 24))
        .find(|&a| name(i, a) == method)
        .unwrap()
}
fn symbol(raw: &[u8], wanted: &str) -> u64 {
    let u32at = |n| u32::from_le_bytes(raw[n..n + 4].try_into().unwrap()) as usize;
    let mut at = 32;
    for _ in 0..u32at(16) {
        if u32at(at) == 2 {
            let (syms, count, strings) = (u32at(at + 8), u32at(at + 12), u32at(at + 16));
            for n in 0..count {
                let s = syms + n * 16;
                let p = strings + u32at(s);
                let end = raw[p..].iter().position(|&b| b == 0).unwrap() + p;
                if &raw[p..end] == wanted.as_bytes() {
                    return u64::from_le_bytes(raw[s + 8..s + 16].try_into().unwrap());
                }
            }
        }
        at += u32at(at + 4);
    }
    panic!("missing original symbol {wanted}");
}
#[test]
fn compiled_same_image_and_dylib_hierarchies_nested_super_calls_and_private_reset() {
    for fmt in ["absolute", "compact"] {
        for mode in ["chained", "legacy"] {
            for split in [0, 1] {
                for ctor in [0, 1] {
                    let (raw, mut libs) = fixtures(fmt, mode, split, ctor);
                    let image = link(&raw, &libs).unwrap();
                    let first = compare(&image, 20000).unwrap();
                    assert_eq!(first.exit, 0);
                    assert_eq!(first.syscalls, 2);
                    assert_eq!(
                        first.stdout,
                        b"Superclass lookup, Super2 guest methods, nesting and ABI OK\n"
                    );
                    let prepared = Prepared::new(&image, false).unwrap();
                    for _ in 0..3 {
                        assert_eq!(prepared.run(20000).unwrap(), first);
                    }
                    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                    for dispatch in [false, true] {
                        let p = Prepared::new_with_dispatch(&image, dispatch).unwrap();
                        for _ in 0..3 {
                            assert_eq!(p.run(20000).unwrap(), first);
                        }
                    }
                    let (child, _, base, _) = identities(&image, &raw);
                    assert_ne!(child, base);
                    libs.reverse();
                    assert_eq!(compare(&link(&raw, &libs).unwrap(), 20000).unwrap(), first);
                }
            }
        }
    }
}
fn load(words: &mut Vec<u32>, r: u32, value: u64) {
    for shift in 0..4 {
        words.push(
            (if shift == 0 { 0xd2800000 } else { 0xf2800000 })
                | shift << 21
                | (((value >> (shift * 16)) & 65535) as u32) << 5
                | r,
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
fn runtime(i: &Image) -> u64 {
    i.code_ranges()
        .flat_map(|(a, n)| (a..a + n).step_by(4))
        .find(|&a| i.instruction(a).unwrap() == SUPER)
        .unwrap()
        - 4
}
fn query(
    image: Image,
    receiver: u64,
    current: u64,
    sel: u64,
    pair: u64,
    isa: u64,
) -> (Image, [u64; 31]) {
    let mut words = vec![0xd10803ff];
    let mut expected = [0; 31];
    for (n, v) in expected.iter_mut().enumerate() {
        *v = 0x1234000000000000 + n as u64;
    }
    expected[0] = pair;
    expected[1] = sel;
    expected[20] = runtime(&image);
    for (n, &v) in expected.iter().enumerate() {
        load(&mut words, n as u32, v);
    }
    words.push(0xeb09013f); // CMP X9,X9 sets Z/C; bridge and leaf IMP retain it.
    expected[30] = 0x1000 + (words.len() as u64 + 1) * 4;
    words.push(0xd63f0280);
    words.extend([0xd2800000, 0xd2800030, 0xd4001001]);
    let mut data = Vec::new();
    for value in [receiver, current, isa, 0] {
        data.extend(value.to_le_bytes());
    }
    expected[0] = receiver;
    expected[17] = current;
    (synthetic(image, words, data), expected)
}
#[test]
fn super_pair_lookup_tail_imp_return_preserves_scalar_state_and_exact_budget() {
    let (raw, libs) = fixtures("absolute", "chained", 0, 0);
    let image = link(&raw, &libs).unwrap();
    let (child, _, base, _) = identities(&image, &raw);
    let sel = selector(&image, base, "abi:b:c:d:e:f:");
    let capture = symbol(&raw, "_abi_capture");
    let (probe, expected) = query(image, 0x3010, child, sel, 0x3000, child);
    let prepared = Prepared::new(&probe, false).unwrap();
    let first = prepared.run(2000).unwrap();
    for n in (0..16).chain(18..31) {
        let actual = u64::from_le_bytes(
            first
                .memory
                .read(capture + n * 8, 8)
                .unwrap()
                .try_into()
                .unwrap(),
        );
        assert_eq!(actual, expected[n as usize], "register {n}");
    }
    assert_eq!(
        u64::from_le_bytes(
            first
                .memory
                .read(capture + 248, 8)
                .unwrap()
                .try_into()
                .unwrap()
        ),
        STACK_BASE + STACK_SIZE as u64 - 512
    );
    assert_eq!(first.cpu.nzcv, 0x60000000);
    assert_eq!(first.syscalls, 1);
    let count = 2000 - first.cpu.remaining;
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    let native = [
        Prepared::new_with_dispatch(&probe, false).unwrap(),
        Prepared::new_with_dispatch(&probe, true).unwrap(),
    ];
    for budget in 0..=count + 1 {
        let out = prepared.run(budget);
        if budget < count {
            assert!(out.as_ref().unwrap_err().contains("budget"));
        } else {
            assert_eq!(out.as_ref().unwrap().exit, 0);
        }
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        for p in &native {
            match (&out, p.run(budget)) {
                (Ok(a), Ok(b)) => assert_eq!(a, &b),
                (Err(a), Err(b)) => assert_eq!(a, &b),
                (a, b) => panic!("budget {budget} mismatch {a:?}/{b:?}"),
            }
        }
    }
}
#[test]
fn super_rejects_bad_pairs_current_receiver_selector_and_foreign_traps() {
    let (raw, libs) = fixtures("absolute", "chained", 0, 0);
    let image = link(&raw, &libs).unwrap();
    let (child, meta, base, bmeta) = identities(&image, &raw);
    let sel = selector(&image, base, "value:");
    for (receiver, current, selector, pair) in [
        (0, child, sel, 0x3000),
        (0x3010, base, sel, 0x3000),
        (0x3010, meta, sel, 0x3000),
        (child, child, sel, 0x3000),
        (0x3018, child, sel, 0x3000),
        (0x3010, child, sel, u64::MAX),
        (0x3010, child, sel, 0x3018),
        (0x3010, child, 0x3000, 0x3000),
        (0x3010, child, selector(&image, child, "nested:"), 0x3000),
    ] {
        let (probe, _) = query(image.clone(), receiver, current, selector, pair, child);
        assert!(
            compare(&probe, 2000).is_err(),
            "{receiver:#x} {current:#x} {selector:#x} {pair:#x}"
        );
    }
    // A super call from a root class method resolves through the root metaclass
    // to the root class's instance methods, as in the real runtime.
    let (probe, _) = query(image.clone(), base, bmeta, sel, 0x3000, child);
    assert!(compare(&probe, 2000).is_ok());
    let mut data = Vec::new();
    data.extend(0x3010u64.to_le_bytes());
    data.extend(u64::MAX.to_le_bytes());
    data.extend(child.to_le_bytes());
    data.extend(0u64.to_le_bytes());
    let mut words = Vec::new();
    load(&mut words, 0, 0x3000);
    load(&mut words, 1, sel);
    load(&mut words, 20, runtime(&image));
    words.push(0xd63f0280);
    assert!(
        compare(&synthetic(image.clone(), words, data), 100)
            .unwrap_err()
            .contains("current class")
    );
    assert_eq!(
        compare(&synthetic(image, vec![SUPER], vec![0; 32]), 100).unwrap_err(),
        "unregistered ObjC trap"
    );
}
fn overwrite(image: &mut Image, address: u64, data: &[u8]) {
    for s in &mut image.segments {
        if let Some(n) = address.checked_sub(s.address).map(|v| v as usize)
            && n.checked_add(data.len())
                .is_some_and(|end| end <= s.bytes.len())
        {
            std::sync::Arc::make_mut(&mut s.bytes)[n..n + data.len()].copy_from_slice(data);
            return;
        }
    }
    panic!("bad mutation");
}
#[test]
fn super_metadata_stub_mutation_and_instruction_aliases_reject() {
    let (raw, libs) = fixtures("absolute", "chained", 0, 0);
    let image = link(&raw, &libs).unwrap();
    let (child, meta, base, _) = identities(&image, &raw);
    let refs = section(&raw, "__objc_superrefs").0;
    let class_name = word(&image, word(&image, child + 32) + 24);
    let sel = selector(&image, base, "value:");
    for address in [refs, child + 8, meta + 8, class_name, runtime(&image)] {
        let mut bad = image.clone();
        overwrite(&mut bad, address, &[1; 8]);
        assert!(Prepared::new(&bad, false).is_err());
    }
    for address in [refs, child + 8, meta + 8] {
        let mut words = Vec::new();
        load(&mut words, 9, address);
        load(&mut words, 10, 1);
        words.push(0xf900012a);
        load(&mut words, 0, 0x3000);
        load(&mut words, 1, sel);
        load(&mut words, 20, runtime(&image));
        words.push(0xd63f0280);
        let mut data = Vec::new();
        for value in [0x3010, child, child, 0] {
            data.extend(value.to_le_bytes());
        }
        assert_eq!(
            compare(&synthetic(image.clone(), words, data), 100).unwrap_err(),
            "mutated ObjC metadata unsupported"
        );
    }
    let mut alias = image;
    alias.extra_text.push((refs, 8));
    assert!(
        Prepared::new(&alias, false)
            .err()
            .unwrap()
            .contains("metadata overlaps")
    );
}
#[test]
fn depth_three_runs_and_protocol_adoption_stays_direct_despite_inherited_methods() {
    for fmt in ["absolute", "compact"] {
        for mode in ["chained", "legacy"] {
            for split in [0, 1] {
                // The former "rejected third level" fixture is within depth eight now.
                let (raw, libs) = fixtures(fmt, mode, split, 2);
                let deep = link(&raw, &libs).unwrap();
                assert_eq!(compare(&deep, 20000).unwrap().exit, 0);
                if split == 0 {
                    let (raw, libs) = fixtures(fmt, mode, 0, 3);
                    assert_eq!(compare(&link(&raw, &libs).unwrap(), 20000).unwrap().exit, 0);
                }
                if split == 1 {
                    let (raw, libs) = fixtures(fmt, mode, split, 0);
                    assert!(link(&raw, &libs[1..]).is_err());
                }
            }
        }
    }
}
