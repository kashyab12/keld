use sable::{
    engine::{Outcome, Prepared},
    macho::{Image, Segment},
};
const CREATE: u32 = 0xd4000001 | (0x5ab8 << 5);
const LOOKUP: u32 = 0xd4000001 | (0x5ab3 << 5);
const BASE: u64 = 0x72000000;
const SLOT: usize = 512;
const SLOTS: usize = 256;
fn fixtures(fmt: &str, mode: &str, profile: u8, split: u8, ctor: u8) -> (Vec<u8>, Vec<Vec<u8>>) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/bin/payload")
        .join(fmt)
        .join(mode)
        .join(profile.to_string())
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
        std::fs::read(root.join(format!("payload-{ctor}.macho"))).unwrap(),
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
            _ => panic!("payload mismatch {result:?}/{other:?}"),
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
        .find(|&p| name(i, word(i, word(i, p + 32) + 24)) == "PayloadChild")
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
fn synthetic(mut image: Image, words: Vec<u32>) -> Image {
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
        address: 0x10000,
        bytes: vec![0; 1024].into(),
        protection: 3,
    });
    image
}
fn service(i: &Image, word: u32) -> u64 {
    i.code_ranges()
        .flat_map(|(a, n)| (a..a + n).step_by(4))
        .find(|&a| i.instruction(a).unwrap() == word)
        .unwrap()
}
fn exit(words: &mut Vec<u32>) {
    words.extend([0xd2800000, 0xd2800030, 0xd4001001]);
}
fn call(words: &mut Vec<u32>, target: u64, a: u64, b: u64) {
    load(words, 0, a);
    load(words, 1, b);
    load(words, 20, target);
    words.push(0xd63f0280);
}
fn linked() -> (Vec<u8>, Image) {
    let (raw, libs) = fixtures("absolute", "chained", 0, 0, 0);
    let image = link(&raw, &libs).unwrap();
    (raw, image)
}
#[test]
fn compiled_fields_setters_super_sums_all_sizes_extra_bounds_tls_ctor_and_reset() {
    for fmt in ["absolute", "compact"] {
        for mode in ["chained", "legacy"] {
            for profile in 0..4 {
                for split in [0, 1] {
                    for ctor in [0, 1] {
                        let (raw, mut libs) = fixtures(fmt, mode, profile, split, ctor);
                        let image = link(&raw, &libs).unwrap();
                        let first = compare(&image, 30000).unwrap();
                        assert_eq!(first.exit, 0);
                        assert_eq!(first.syscalls, 2);
                        assert_eq!(first.stdout,b"Payload fields, void setters, super sums, extra bounds and reset OK\n");
                        assert!(
                            first
                                .memory
                                .read(BASE, SLOT * SLOTS)
                                .unwrap()
                                .iter()
                                .all(|&b| b == 0)
                        );
                        let p = Prepared::new(&image, false).unwrap();
                        for _ in 0..3 {
                            assert_eq!(p.run(30000).unwrap(), first);
                        }
                        for budget in [0, 1, 20, 200] {
                            assert!(p.run(budget).unwrap_err().contains("budget"));
                            assert_eq!(p.run(30000).unwrap(), first);
                        }
                        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                        for dispatch in [false, true] {
                            let p = Prepared::new_with_dispatch(&image, dispatch).unwrap();
                            for _ in 0..3 {
                                assert_eq!(p.run(30000).unwrap(), first);
                            }
                        }
                        libs.reverse();
                        assert_eq!(compare(&link(&raw, &libs).unwrap(), 30000).unwrap(), first);
                    }
                }
            }
        }
    }
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
    panic!("bad patch");
}
#[test]
fn payload_cells_sizes_and_names_reject_prepared_live_changes_and_code_aliases() {
    let (raw, image) = linked();
    let (child, _, _, _) = identities(&image, &raw);
    let ro = word(&image, child + 32);
    let rows = word(&image, ro + 48);
    let cell = word(&image, rows + 8);
    let name = word(&image, rows + 16);
    let getter = selector(&image, child, "child");
    let create = service(&image, CREATE);
    let send = service(&image, LOOKUP) - 4;
    for a in [cell, ro + 8, name] {
        let mut bad = image.clone();
        overwrite(&mut bad, a, &[1; 4]);
        assert!(Prepared::new(&bad, false).is_err());
    }
    for a in [cell, ro + 8] {
        let mut w = Vec::new();
        call(&mut w, create, child, 64);
        load(&mut w, 9, a);
        load(&mut w, 10, 32);
        w.push(0xb900012a);
        call(&mut w, send, BASE, getter);
        assert_eq!(
            compare(&synthetic(image.clone(), w), 200).unwrap_err(),
            "mutated ObjC metadata unsupported"
        );
    }
    for a in [cell, rows, ro] {
        let mut alias = image.clone();
        alias.extra_text.push((a, 4));
        assert!(
            Prepared::new(&alias, false)
                .err()
                .unwrap()
                .contains("metadata overlaps")
        );
    }
}
#[test]
fn real_void_setter_side_effects_and_each_lookup_store_return_budget_edge() {
    let (raw, image) = linked();
    let (child, _, _, _) = identities(&image, &raw);
    let create = service(&image, CREATE);
    let send = service(&image, LOOKUP) - 4;
    let setter = selector(&image, child, "setChild:");
    let mut w = Vec::new();
    call(&mut w, create, child, 64);
    load(&mut w, 1, setter);
    load(&mut w, 2, 25);
    load(&mut w, 20, send);
    w.push(0xd63f0280); // Ignore the unspecified setter result; validate the actual guest store.
    exit(&mut w);
    let probe = synthetic(image, w);
    let p = Prepared::new(&probe, false).unwrap();
    let first = p.run(2000).unwrap();
    assert_eq!(
        u64::from_le_bytes(first.memory.read(BASE + 16, 8).unwrap().try_into().unwrap()),
        25
    );
    assert_eq!(
        u64::from_le_bytes(first.memory.read(BASE + 8, 8).unwrap().try_into().unwrap()),
        0
    );
    let count = 2000 - first.cpu.remaining;
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    let native = [
        Prepared::new_with_dispatch(&probe, false).unwrap(),
        Prepared::new_with_dispatch(&probe, true).unwrap(),
    ];
    for budget in 0..=count + 1 {
        let out = p.run(budget);
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
                (a, b) => panic!("budget{budget}:{a:?}/{b:?}"),
            }
        }
    }
}
