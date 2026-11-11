use sable::{
    engine::{Outcome, Prepared},
    macho::{Image, Segment},
};
fn inputs(format: &str, mode: &str, client: &str) -> (Vec<u8>, Vec<Vec<u8>>) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("fixtures/bin/ownership/{format}/{mode}"));
    (
        std::fs::read(dir.join(format!("{client}.macho"))).unwrap(),
        [
            "CoreFoundation.dylib",
            "Foundation.dylib",
            "libsable_objc.dylib",
            "libsable_system.dylib",
        ]
        .map(|n| std::fs::read(dir.join(n)).unwrap())
        .into(),
    )
}
fn link(raw: &[u8], libs: &[Vec<u8>]) -> sable::Result<Image> {
    Image::link_main(raw, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>())
}
fn patch(image: &mut Image, a: u64, data: &[u8]) {
    let s = image
        .segments
        .iter_mut()
        .find(|s| a >= s.address && a + data.len() as u64 <= s.address + s.bytes.len() as u64)
        .unwrap();
    std::sync::Arc::make_mut(&mut s.bytes)
        [(a - s.address) as usize..(a - s.address) as usize + data.len()]
        .copy_from_slice(data);
}
fn compare(image: &Image) -> Outcome {
    let first = Prepared::new(image, false).unwrap().run(30000).unwrap();
    let p = Prepared::new(image, false).unwrap();
    for _ in 0..3 {
        assert_eq!(p.run(30000).unwrap(), first);
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    for dispatch in [false, true] {
        let p = Prepared::new_with_dispatch(image, dispatch).unwrap();
        for _ in 0..3 {
            assert_eq!(p.run(30000).unwrap(), first);
        }
    }
    first
}
#[test]
fn original_arc_transform_and_nested_ownership_matrix_private_state() {
    for format in ["absolute", "compact"] {
        for mode in ["legacy", "chained"] {
            for client in ["client", "observer"] {
                let (raw, mut libs) = inputs(format, mode, client);
                let image = link(&raw, &libs).unwrap();
                let result = compare(&image);
                assert_eq!(result.exit, 0);
                assert_eq!(
                    result.stdout,
                    if client == "client" {
                        b"SABLE FOUNDATION\n".as_slice()
                    } else {
                        b"Ownership, pools, guest dealloc, x20, ctor/TLV OK\n".as_slice()
                    }
                );
                // Success is also gated on zero owned objects, pools, queue and callbacks by Process::finish.
                assert!(
                    result
                        .memory
                        .read(0x72000000, 256 * 512)
                        .unwrap()
                        .iter()
                        .all(|&v| v == 0)
                );
                libs.reverse();
                assert_eq!(compare(&link(&raw, &libs).unwrap()), result);
            }
        }
    }
}
#[test]
fn every_budget_prefix_including_service_and_callback_returns_resets_privately() {
    for client in ["client", "observer"] {
        let (raw, libs) = inputs("absolute", "chained", client);
        let image = link(&raw, &libs).unwrap();
        let p = Prepared::new(&image, false).unwrap();
        let first = p.run(30000).unwrap();
        let used = 30000 - first.cpu.remaining;
        for budget in 0..used {
            assert!(
                p.run(budget).unwrap_err().contains("budget"),
                "budget {budget}"
            );
        }
        let at_edge = p.run(used).unwrap();
        assert_eq!(at_edge.cpu.remaining, 0);
        assert_eq!(at_edge.stdout, first.stdout);
        assert_eq!(at_edge.memory, first.memory);
        assert_eq!(p.run(30000).unwrap(), first);
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        for dispatch in [false, true] {
            let p = Prepared::new_with_dispatch(&image, dispatch).unwrap();
            for budget in 0..used {
                assert!(
                    p.run(budget).unwrap_err().contains("budget"),
                    "budget {budget} dispatch {dispatch}"
                );
            }
            assert_eq!(p.run(used).unwrap(), at_edge);
            assert_eq!(p.run(30000).unwrap(), first);
        }
    }
}
#[test]
fn explicit_main_entry_preserves_freestanding_policy_and_actual_arguments() {
    let (raw, libs) = inputs("absolute", "chained", "client");
    let default = Image::link(&raw, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
    assert!(default.startup.is_none());
    assert!(Prepared::new(&default, false).unwrap().run(30000).is_err());
    assert_eq!(compare(&link(&raw, &libs).unwrap()).exit, 0);
}
#[test]
fn complete_bridges_stub_origin_actual_getter_transform_and_data_are_required() {
    let (raw, libs) = inputs("absolute", "chained", "client");
    let image = link(&raw, &libs).unwrap();
    // Remove any one export from a complete set; runtime discovery must reject the partial set.
    for name in [
        "_objc_autoreleasePoolPush",
        "_objc_autoreleasePoolPop",
        "_objc_retain",
        "_objc_release",
        "_objc_autorelease",
        "_objc_retainAutorelease",
        "_objc_claimAutoreleasedReturnValue",
        "_objc_retainAutoreleasedReturnValue",
        "_objc_release_x20",
        "_objc_release_x21",
        "_objc_release_x22",
        "_objc_release_x23",
        "_objc_release_x24",
        "_objc_retain_x22",
    ] {
        let mut bad = libs.clone();
        let at = export_last_edge(&bad[2], name);
        bad[2][at] = b'X';
        assert!(link(&raw, &bad).is_err(), "partial {name}");
    }
    let mut wrong_hook = libs.clone();
    let mut changed = false;
    for at in 0..wrong_hook[1].len() - 8 {
        if &wrong_hook[1][at..at + 8] == b"v16@0:8\0" {
            wrong_hook[1][at] = b'Q';
            changed = true;
        }
    }
    assert!(changed);
    assert!(
        link(&raw, &wrong_hook)
            .unwrap_err()
            .contains("dealloc signature")
    );
    let own = (0xd4000001u32 | (0x5ac2 << 5)).to_le_bytes();
    let retain = image
        .code_ranges()
        .flat_map(|(a, n)| (a..a + n).step_by(4))
        .find(|&a| image.instruction(a).unwrap().to_le_bytes() == own)
        .unwrap();
    let mut bad = image.clone();
    patch(&mut bad, retain + 4, &0xd503201fu32.to_le_bytes());
    assert!(Prepared::new(&bad, false).is_err());
    // A valid private SVC at another address must not impersonate the registered bridge.
    let mut bad = image.clone();
    bad.entry = 0x1000;
    bad.startup = None;
    bad.extra_text.push((0x1000, 4));
    bad.segments.push(Segment {
        address: 0x1000,
        bytes: own.to_vec().into(),
        protection: 5,
    });
    assert!(
        Prepared::new(&bad, false)
            .unwrap()
            .run(100)
            .unwrap_err()
            .contains("unregistered ownership trap")
    );
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
    assert!(
        Prepared::new(&bad, false)
            .unwrap()
            .run(30000)
            .unwrap_err()
            .contains("memory fault")
    );
    // Alter the actual scalar ASCII transformation, while leaving all metadata/getter intact.
    let transform = image
        .code_ranges()
        .flat_map(|(a, n)| (a..a + n).step_by(4))
        .find(|&a| {
            matches!(
                sable::isa::decode(image.instruction(a).unwrap(), a),
                Ok(sable::isa::Op::AddImm {
                    value: 32,
                    subtract: true,
                    wide: false,
                    ..
                })
            )
        })
        .unwrap();
    let original = image.instruction(transform).unwrap();
    let mut bad = image.clone();
    patch(
        &mut bad,
        transform,
        &(original & !(0xfff << 10)).to_le_bytes(),
    );
    assert_ne!(
        Prepared::new(&bad, false).unwrap().run(30000).unwrap().exit,
        0
    );
    // Constant input corruption is caught by the prepared snapshots.
    let at = image
        .segments
        .iter()
        .find_map(|s| {
            s.bytes
                .windows(17)
                .position(|b| b == b"sable foundation\0")
                .map(|n| s.address + n as u64)
        })
        .unwrap();
    let mut bad = image.clone();
    patch(&mut bad, at, b"X");
    assert!(Prepared::new(&bad, false).is_err());
}

fn export_last_edge(raw: &[u8], target: &str) -> usize {
    fn u32at(raw: &[u8], i: usize) -> usize {
        u32::from_le_bytes(raw[i..i + 4].try_into().unwrap()) as usize
    }
    fn leb(raw: &[u8], p: &mut usize) -> usize {
        let mut v = 0;
        let mut shift = 0;
        loop {
            let b = raw[*p];
            *p += 1;
            v |= ((b & 127) as usize) << shift;
            if b < 128 {
                return v;
            }
            shift += 7;
        }
    }
    fn walk(
        raw: &[u8],
        base: usize,
        offset: usize,
        prefix: &str,
        last: usize,
        target: &str,
    ) -> Option<usize> {
        let mut p = base + offset;
        let terminal = leb(raw, &mut p);
        if terminal > 0 && prefix == target {
            return Some(last);
        }
        p += terminal;
        let count = raw[p];
        p += 1;
        for _ in 0..count {
            let edge = p;
            while raw[p] != 0 {
                p += 1;
            }
            let name = std::str::from_utf8(&raw[edge..p]).unwrap();
            p += 1;
            let child = leb(raw, &mut p);
            if let Some(a) = walk(raw, base, child, &format!("{prefix}{name}"), edge, target) {
                return Some(a);
            }
        }
        None
    }
    let mut p = 32;
    let mut base = None;
    for _ in 0..u32at(raw, 16) {
        match u32at(raw, p) {
            0x80000033 => base = Some(u32at(raw, p + 8)),
            0x80000022 => base = Some(u32at(raw, p + 40)),
            _ => {}
        }
        p += u32at(raw, p + 4);
    }
    walk(raw, base.unwrap(), 0, "", 0, target).unwrap()
}
