use sable::{
    engine::{Prepared, run},
    macho::Image,
};

const SHORT: &[u8] = include_bytes!("../fixtures/bin/dynamic/dynamic-8.macho");
const LONG: &[u8] = include_bytes!("../fixtures/bin/dynamic/dynamic-2000.macho");
const MATH: &[u8] = include_bytes!("../fixtures/bin/dynamic/libsable_math.dylib");
const HELPER: &[u8] = include_bytes!("../fixtures/bin/dynamic/libsable_helper.dylib");

const SYSTEM: &[u8] = include_bytes!("../fixtures/bin/dynamic/libsable_system.dylib");

#[test]
fn transitive_binding_rebasing_and_private_state() {
    for (main, checksum) in [(SHORT, 0x38afd5babu64), (LONG, 0xefe1edd5865163fb)] {
        let image = Image::link(main, &[MATH, HELPER, SYSTEM]).unwrap();
        let initial = image.segments.clone();
        assert_eq!(image.code_ranges().count(), 4);
        assert!(
            image
                .code_ranges()
                .all(|(start, _)| image.contains_pc(start))
        );
        let prepared = Prepared::new(&image, false).unwrap();
        let first = prepared.run(1_000_000).unwrap();
        assert_eq!(first.stdout, checksum.to_le_bytes());
        assert_eq!(first.exit, 0);
        assert_eq!(prepared.run(1_000_000).unwrap(), first);
        assert_eq!(image.segments, initial);
        assert!(first.memory.shared_image_bytes() > 32 * 1024);
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            for native in [false, true] {
                let prepared = Prepared::new_with_dispatch(&image, native).unwrap();
                for _ in 0..3 {
                    assert_eq!(prepared.run(1_000_000).unwrap(), first);
                }
            }
        }
    }
}

#[test]
fn supplied_order_does_not_change_two_level_resolution() {
    let a = Image::link(SHORT, &[MATH, HELPER, SYSTEM]).unwrap();
    let b = Image::link(SHORT, &[HELPER, SYSTEM, MATH]).unwrap();
    assert_eq!(
        run(&a, 10000, false).unwrap(),
        run(&b, 10000, false).unwrap()
    );
}

#[test]
fn missing_duplicate_and_wrong_kind_dependencies_fail_closed() {
    assert!(Image::parse(SHORT).is_err());
    assert!(
        Image::link(SHORT, &[])
            .unwrap_err()
            .contains("not supplied")
    );
    assert!(
        Image::link(SHORT, &[MATH])
            .unwrap_err()
            .contains("not supplied")
    );
    assert!(
        Image::link(SHORT, &[MATH, MATH, HELPER])
            .unwrap_err()
            .contains("duplicate")
    );
    assert!(Image::link(MATH, &[HELPER]).is_err());
    assert!(Image::link(SHORT, &[SHORT]).is_err());
    for offset in [0, 1, 31, 32, 512, MATH.len() - 1] {
        assert!(Image::link(SHORT, &[&MATH[..offset], HELPER, SYSTEM]).is_err());
    }
}

#[test]
fn corrupted_import_and_version_are_not_silently_accepted() {
    let mut wrong_symbol = MATH.to_vec();
    let mut changed = false;
    // Both export-trie and string-table occurrences are changed; no symbol fallback.
    for i in 0..wrong_symbol.len() - 10 {
        if &wrong_symbol[i..i + 10] == b"_sable_mix" {
            wrong_symbol[i + 7] = b'X';
            changed = true;
        }
    }
    assert!(changed);
    assert!(
        Image::link(SHORT, &[&wrong_symbol, HELPER, SYSTEM])
            .unwrap_err()
            .contains("unresolved import")
    );
    let mut wrong_version = MATH.to_vec();
    let mut at = 32;
    let count = u32::from_le_bytes(wrong_version[16..20].try_into().unwrap());
    for _ in 0..count {
        let cmd = u32::from_le_bytes(wrong_version[at..at + 4].try_into().unwrap());
        if cmd == 0xd {
            wrong_version[at + 16..at + 20].fill(0);
        }
        at += u32::from_le_bytes(wrong_version[at + 4..at + 8].try_into().unwrap()) as usize;
    }
    assert!(
        Image::link(SHORT, &[&wrong_version, HELPER, SYSTEM])
            .unwrap_err()
            .contains("version")
    );
}

#[test]
fn all_initializer_and_tls_section_forms_are_rejected() {
    for kind in [9, 10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16] {
        let mut bytes = SHORT.to_vec();
        let mut at = 32;
        let mut replaced = false;
        let commands = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        for _ in 0..commands {
            let cmd = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
            if cmd == 0x19 {
                let count = u32::from_le_bytes(bytes[at + 64..at + 68].try_into().unwrap());
                for i in 0..count as usize {
                    let s = at + 72 + i * 80;
                    let flags = u32::from_le_bytes(bytes[s + 64..s + 68].try_into().unwrap());
                    let size = u64::from_le_bytes(bytes[s + 40..s + 48].try_into().unwrap());
                    if flags & 0x80000400 == 0 && size > 0 {
                        bytes[s + 64..s + 68]
                            .copy_from_slice(&((flags & !255) | kind).to_le_bytes());
                        replaced = true;
                        break;
                    }
                }
            }
            if replaced {
                break;
            }
            at += u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        }
        assert!(replaced);
        assert!(
            Image::link(&bytes, &[MATH, HELPER, SYSTEM])
                .unwrap_err()
                .contains("initializers")
        );
    }
}
