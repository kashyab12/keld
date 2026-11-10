use sable::{engine::Prepared, macho::Image};

#[test]
fn llvm_chains_execute_with_rebases_transitive_imports_and_addends() {
    let math = include_bytes!("../fixtures/bin/chained/libsable_math.dylib").as_slice();
    let helper = include_bytes!("../fixtures/bin/chained/libsable_helper.dylib").as_slice();
    let system = include_bytes!("../fixtures/bin/chained/libsable_system.dylib").as_slice();
    let numbers = include_bytes!("../fixtures/bin/chained/libsable_numbers.dylib").as_slice();
    let cases = [
        (
            include_bytes!("../fixtures/bin/chained/dynamic-8.macho").as_slice(),
            0x38afd5babu64,
            vec![math, helper, system],
        ),
        (
            include_bytes!("../fixtures/bin/chained/dynamic-2000.macho").as_slice(),
            0xefe1edd5865163fb,
            vec![math, helper, system],
        ),
        (
            include_bytes!("../fixtures/bin/chained/dynamic-200000.macho").as_slice(),
            0xa7151818301809c9,
            vec![math, helper, system],
        ),
        (
            include_bytes!("../fixtures/bin/chained/chained-addend.macho").as_slice(),
            51,
            vec![numbers, system],
        ),
    ];
    for (main, checksum, mut libraries) in cases {
        assert!(Image::parse(main).is_err());
        assert!(Image::link(main, &[]).is_err());
        let image = Image::link(main, &libraries).unwrap();
        let initial = image.segments.clone();
        let prepared = Prepared::new(&image, false).unwrap();
        let first = prepared.run(10_000_000).unwrap();
        assert_eq!(first.stdout, checksum.to_le_bytes());
        assert_eq!(first.exit, 0);
        assert_eq!(prepared.run(10_000_000).unwrap(), first);
        assert_eq!(image.segments, initial);
        libraries.reverse();
        let reordered = Image::link(main, &libraries).unwrap();
        assert_eq!(
            Prepared::new(&reordered, false)
                .unwrap()
                .run(10_000_000)
                .unwrap(),
            first
        );
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        for native in [false, true] {
            let prepared = Prepared::new_with_dispatch(&image, native).unwrap();
            assert_eq!(prepared.run(10_000_000).unwrap(), first);
            assert_eq!(prepared.run(10_000_000).unwrap(), first);
        }
    }
}
