use sable::{
    engine::{Outcome, Prepared},
    macho::{Image, Segment},
    memory::{STACK_BASE, STACK_SIZE},
};
const CREATE: u32 = 0xd4000001 | (0x5ab8 << 5);
const DISPOSE: u32 = 0xd4000001 | (0x5ab9 << 5);
const LOOKUP: u32 = 0xd4000001 | (0x5ab3 << 5);
const SUPER: u32 = 0xd4000001 | (0x5ab7 << 5);
const BASE: u64 = 0x72000000;
const SLOT: usize = 512;
const SLOTS: usize = 256;
fn fixtures(fmt: &str, mode: &str, split: u8, ctor: u8) -> (Vec<u8>, Vec<Vec<u8>>) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/bin/instances")
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
        std::fs::read(root.join(format!("instance-{ctor}.macho"))).unwrap(),
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
            _ => panic!("instance mismatch {result:?}/{other:?}"),
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
        .find(|&p| name(i, word(i, word(i, p + 32) + 24)) == "AllocChild")
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
    let (raw, libs) = fixtures("absolute", "chained", 0, 0);
    let image = link(&raw, &libs).unwrap();
    (raw, image)
}
#[test]
fn compiled_pointer_methods_instances_zeroing_isolation_tls_ctor_and_private_reset() {
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
                        b"Guest instances, pointer methods, zeroing, isolation and disposal OK\n"
                    );
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
                        assert_eq!(p.run(20000).unwrap(), first);
                    }
                    for budget in [0, 1, 20, 100] {
                        assert!(p.run(budget).unwrap_err().contains("budget"));
                        assert_eq!(p.run(20000).unwrap(), first);
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
}
#[test]
fn creation_and_disposal_preserve_scalar_state_and_each_svc_ret_budget_edge() {
    let (raw, image) = linked();
    let (child, _, _, _) = identities(&image, &raw);
    let create = service(&image, CREATE);
    let dispose = service(&image, DISPOSE);
    let mut words = vec![0xd11003ff]; // SUB SP,SP,#1024.
    let mut expected = [0u64; 31];
    for (n, v) in expected.iter_mut().enumerate() {
        *v = 0x1234000000000000 + n as u64;
    }
    expected[0] = child;
    expected[1] = 64;
    expected[19] = create;
    expected[20] = dispose;
    for (n, &v) in expected.iter().enumerate() {
        load(&mut words, n as u32, v);
    }
    words.push(0xeb09013f);
    let resume_create = 0x1000 + (words.len() as u64 + 1) * 4;
    words.push(0xd63f0260);
    for r in 0..31 {
        words.push(0xf9000000 | r << 10 | 31 << 5 | r);
    }
    words.extend([0x910003e0, 0xf9007fe0]);
    load(&mut words, 0, BASE);
    let resume_dispose = 0x1000 + (words.len() as u64 + 1) * 4;
    words.push(0xd63f0280);
    for r in 0..31 {
        words.push(0xf9000000 | (32 + r) << 10 | 31 << 5 | r);
    }
    words.extend([0x910003e0, 0xf900ffe0]);
    exit(&mut words);
    let probe = synthetic(image, words);
    let p = Prepared::new(&probe, false).unwrap();
    let first = p.run(2000).unwrap();
    let sp = STACK_BASE + STACK_SIZE as u64 - 1024;
    for (offset, result, lr) in [(0, BASE, resume_create), (256, 0, resume_dispose)] {
        for r in 0..31 {
            let actual = u64::from_le_bytes(
                first
                    .memory
                    .read(sp + offset + r * 8, 8)
                    .unwrap()
                    .try_into()
                    .unwrap(),
            );
            assert_eq!(
                actual,
                match r {
                    0 => result,
                    30 => lr,
                    _ => expected[r as usize],
                },
                "r{r},offset{offset}"
            );
        }
        assert_eq!(
            u64::from_le_bytes(
                first
                    .memory
                    .read(sp + offset + 248, 8)
                    .unwrap()
                    .try_into()
                    .unwrap()
            ),
            sp
        );
    }
    assert_eq!(first.cpu.nzcv, 0x60000000);
    assert_eq!(first.syscalls, 1);
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
                (a, b) => panic!("budget {budget}: {a:?}/{b:?}"),
            }
        }
    }
}
#[test]
fn invalid_classes_sizes_ownership_isa_and_stale_message_identities_reject() {
    let (raw, image) = linked();
    let (child, meta, _, _) = identities(&image, &raw);
    let create = service(&image, CREATE);
    let dispose = service(&image, DISPOSE);
    let lookup = service(&image, LOOKUP) - 4;
    let super_call = service(&image, SUPER) - 4;
    let sel = selector(&image, child, "value:");
    for (a, b) in [
        (meta, 0),
        (child + 8, 0),
        (u64::MAX, 0),
        (child, 65),
        (child, u64::MAX),
    ] {
        let mut w = Vec::new();
        call(&mut w, create, a, b);
        assert!(compare(&synthetic(image.clone(), w), 500).is_err());
    }
    for address in [child, 0x10000, BASE + 8, BASE + SLOT as u64, u64::MAX] {
        let mut w = Vec::new();
        if address == 0x10000 {
            load(&mut w, 9, address);
            load(&mut w, 10, child);
            w.push(0xf900012a);
        }
        call(&mut w, dispose, address, 0);
        assert!(compare(&synthetic(image.clone(), w), 500).is_err());
    }
    let mut nil = Vec::new();
    call(&mut nil, create, 0, u64::MAX);
    nil.push(0xaa0003e9);
    call(&mut nil, dispose, 0, 0);
    exit(&mut nil);
    let out = compare(&synthetic(image.clone(), nil), 500).unwrap();
    assert_eq!(out.cpu.x[9], 0);
    for variant in 0..6 {
        let mut w = Vec::new();
        call(&mut w, create, child, 16);
        if variant == 0 {
            load(&mut w, 9, BASE);
            load(&mut w, 10, meta);
            w.push(0xf900012a);
            call(&mut w, dispose, BASE, 0);
        } else if variant == 1 {
            call(&mut w, dispose, BASE + 8, 0);
        } else {
            call(&mut w, dispose, BASE, 0);
            if variant == 2 {
                call(&mut w, dispose, BASE, 0);
            } else {
                load(&mut w, 9, BASE);
                load(&mut w, 10, child);
                w.push(0xf900012a);
                if variant == 3 {
                    call(&mut w, lookup, BASE, sel);
                } else if variant == 4 {
                    load(&mut w, 9, 0x10000);
                    load(&mut w, 10, BASE);
                    w.push(0xf900012a);
                    load(&mut w, 10, child);
                    w.push(0xf900052a);
                    call(&mut w, super_call, 0x10000, sel);
                } else {
                    call(&mut w, lookup, BASE + SLOT as u64, sel);
                }
            }
        }
        assert!(
            compare(&synthetic(image.clone(), w), 500).is_err(),
            "variant{variant}"
        );
    }
}
#[test]
fn saturation_returns_nil_without_reusing_retired_slots_or_resizing_memory() {
    let (raw, image) = linked();
    let (child, _, _, _) = identities(&image, &raw);
    let create = service(&image, CREATE);
    let dispose = service(&image, DISPOSE);
    let mut w = Vec::new();
    for _ in 0..SLOTS {
        call(&mut w, create, child, 64);
    }
    call(&mut w, create, child, 0);
    w.push(0xaa0003e9);
    call(&mut w, dispose, BASE, 0);
    call(&mut w, create, child, 0);
    w.push(0xaa0003ea);
    exit(&mut w);
    let probe = synthetic(image, w);
    let first = compare(&probe, 20000).unwrap();
    assert_eq!(first.cpu.x[9], 0);
    assert_eq!(first.cpu.x[10], 0);
    assert!(
        first
            .memory
            .read(BASE, SLOT)
            .unwrap()
            .iter()
            .all(|&b| b == 0)
    );
    for n in 1..SLOTS {
        let at = BASE + (n * SLOT) as u64;
        assert_eq!(
            u64::from_le_bytes(first.memory.read(at, 8).unwrap().try_into().unwrap()),
            child
        );
        assert!(
            first
                .memory
                .read(at + 8, 64)
                .unwrap()
                .iter()
                .all(|&b| b == 0)
        );
    }
    let p = Prepared::new(&probe, false).unwrap();
    assert_eq!(p.run(20000).unwrap(), first);
}
#[test]
fn pointer_arguments_remain_guest_values_and_foreign_or_mutated_services_reject() {
    let (raw, image) = linked();
    let (child, _, base, _) = identities(&image, &raw);
    let create = service(&image, CREATE);
    let dispose = service(&image, DISPOSE);
    let send = service(&image, LOOKUP) - 4;
    let sel = selector(&image, base, "choose:");
    let mut w = Vec::new();
    call(&mut w, create, child, 0);
    load(&mut w, 1, sel);
    load(&mut w, 2, 0x1234567890);
    load(&mut w, 20, send);
    w.push(0xd63f0280);
    w.push(0xaa0003e9);
    exit(&mut w);
    assert_eq!(
        compare(&synthetic(image.clone(), w), 500).unwrap().cpu.x[9],
        0x1234567890
    );
    for word in [CREATE, DISPOSE] {
        assert_eq!(
            compare(&synthetic(image.clone(), vec![word]), 100).unwrap_err(),
            "unregistered ObjC trap"
        );
    }
    for address in [create, dispose] {
        let mut bad = image.clone();
        for s in &mut bad.segments {
            if let Some(n) = address.checked_sub(s.address).map(|n| n as usize)
                && n + 4 <= s.bytes.len()
            {
                std::sync::Arc::make_mut(&mut s.bytes)[n..n + 4]
                    .copy_from_slice(&0xd503201fu32.to_le_bytes());
                break;
            }
        }
        assert!(Prepared::new(&bad, false).is_err());
    }
}
