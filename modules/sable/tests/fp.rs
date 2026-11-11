//! FP/SIMD state, HFA calling convention and rejection checks.
use sable::{
    engine::{Cpu, Prepared, step_memory},
    macho::{Image, Segment},
    memory::Memory,
};

const EXPECTED: &[u8] = include_bytes!("../fixtures/fp/hfa-expected.txt");
const SYSTEM: &[u8] = include_bytes!("../fixtures/bin/fp/libsable_system.dylib");

fn program(words: &[u32]) -> Image {
    Image {
        segments: vec![
            Segment {
                address: 0x1000,
                bytes: words
                    .iter()
                    .flat_map(|w| w.to_le_bytes())
                    .collect::<Vec<_>>()
                    .into(),
                protection: 5,
            },
            Segment {
                address: 0x20000,
                bytes: vec![0; 64].into(),
                protection: 3,
            },
        ],
        entry: 0x1000,
        text: (0x1000, words.len() as u64 * 4),
        extra_text: Vec::new(),
        platform: 7,
        tls: Vec::new(),
        objc: None,
        startup: None,
    }
}

#[test]
fn hfa_fixture_is_byte_exact_at_o0_and_os() {
    for main in [
        &include_bytes!("../fixtures/bin/fp/hfa-O0.macho")[..],
        &include_bytes!("../fixtures/bin/fp/hfa-Os.macho")[..],
    ] {
        let image = Image::link_main(main, &[SYSTEM]).unwrap();
        let reference = Prepared::new(&image, false)
            .unwrap()
            .run(5_000_000)
            .unwrap();
        assert_eq!(reference.exit, 0);
        assert_eq!(reference.stdout, EXPECTED);
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        for native in [false, true] {
            let prepared = Prepared::new_with_dispatch(&image, native).unwrap();
            // Full CPU (including V/FPSR), output and memory parity with the interpreter.
            assert_eq!(prepared.run(5_000_000).unwrap(), reference);
            let compiled = sable::jit::Compiled::new_with_dispatch(&image, native).unwrap();
            assert!(!compiled.declined_fp().is_empty());
            for &pc in compiled.declined_fp() {
                let op = sable::isa::decode(image.instruction(pc).unwrap(), pc).unwrap();
                assert!(matches!(op, sable::isa::Op::Fp(_)), "{pc:#x}");
            }
        }
    }
}

#[test]
fn unsupported_fp_opcodes_are_rejected_by_name_without_state_change() {
    // FMAX d0, d1, d2 is a real scalar FP instruction outside the subset.
    let image = program(&[0x1e62_4820, 0xd400_1001]);
    let mut memory = Memory::new(&image).unwrap();
    let mut cpu = Cpu::new(image.entry, 10);
    cpu.v[1] = 1f64.to_bits() as u128;
    let before = cpu.clone();
    let err = step_memory(&mut cpu, &image, &mut memory).unwrap_err();
    assert!(
        err.contains("unsupported FP/SIMD instruction FMAX"),
        "{err}"
    );
    assert_eq!(cpu, before);
    for (word, name) in [
        (0x4e20_8400u32, "Advanced SIMD three same"),
        (0x1e66_4000, "FRINTA"),
        (0x4e08_1c00, "Advanced SIMD copy"),
        (0x6c00_07e0, "LDNP/STNP"),
        (0x1ee2_2800, "half-precision"),
    ] {
        let err = sable::isa::decode(word, 0x1000).unwrap_err();
        assert!(err.contains(name), "{word:#x}: {err}");
    }
}

#[test]
fn fpcr_accepts_only_round_to_nearest_defaults() {
    // MSR FPCR, X0; MRS X1, FPCR; SVC
    let image = program(&[0xd51b_4400, 0xd53b_4401, 0xd400_1001]);
    for bad in [1u64 << 22, 2 << 22, 1 << 24, 1 << 25, 1 << 8, 1 << 26] {
        let mut memory = Memory::new(&image).unwrap();
        let mut cpu = Cpu::new(image.entry, 10);
        cpu.x[0] = bad;
        let before = cpu.clone();
        let err = step_memory(&mut cpu, &image, &mut memory).unwrap_err();
        assert!(err.contains("MSR FPCR"), "{err}");
        assert_eq!(cpu, before);
    }
    let mut memory = Memory::new(&image).unwrap();
    let mut cpu = Cpu::new(image.entry, 10);
    cpu.x[1] = 7;
    assert!(!step_memory(&mut cpu, &image, &mut memory).unwrap());
    assert!(!step_memory(&mut cpu, &image, &mut memory).unwrap());
    assert_eq!((cpu.fpcr, cpu.x[1]), (0, 0));
}
