use sable::{
    engine::{Cpu, run, step},
    isa::{Op, decode},
    macho::{Image, Segment},
};

fn image(words: &[u32]) -> Image {
    Image {
        segments: vec![Segment {
            address: 0x1000,
            bytes: words
                .iter()
                .flat_map(|w| w.to_le_bytes())
                .collect::<Vec<_>>()
                .into(),
            protection: 5,
        }],
        entry: 0x1000,
        text: (0x1000, words.len() as u64 * 4),
        extra_text: Vec::new(),
        platform: 7,
    }
}
fn movz(dst: u32, value: u32) -> u32 {
    0xd2800000 | value << 5 | dst
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn exit() -> [u32; 3] {
    [movz(0, 0), movz(16, 1), 0xd4001001]
}

#[test]
fn original_linked_simulator_binary() {
    let program = Image::parse(include_bytes!("../fixtures/bin/hello.macho")).unwrap();
    let result = run(&program, 100, false).unwrap();
    assert_eq!(program.platform, 7);
    assert_eq!(result.stdout, b"SABLE_ARM64_DARWIN_OK\n");
    assert_eq!(result.exit, 0);
    assert_eq!(result.syscalls, 2);
    assert_eq!(result.cpu.remaining, 91);
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    assert_eq!(run(&program, 100, true).unwrap(), result);
}

#[test]
fn original_compute_checksum() {
    let program = Image::parse(include_bytes!("../fixtures/bin/compute.macho")).unwrap();
    let result = run(&program, 40_000_000, false).unwrap();
    assert_eq!(result.cpu.x[9], 10_000_000 * 10_000_001 / 2);
    assert_eq!(result.cpu.remaining, 40_000_000 - 30_000_006);
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    assert_eq!(run(&program, 40_000_000, true).unwrap(), result);
}

#[test]
fn architecture_rejects_unimplemented_and_reserved_ops() {
    for word in [
        0x8bc10400, // reserved arithmetic shift=3
        0x0b018000, // reserved W shift amount=32
        0xd503233f,
        0x14000000u32.wrapping_sub(1),
        0xd4000001,
    ] {
        assert!(decode(word, 0x1000).is_err(), "{word:#x}");
    }
    assert_eq!(
        decode(0x17ffffff, 0x1000).unwrap(),
        Op::Branch {
            target: 0xffc,
            link: false
        }
    );
}

#[test]
fn wrapping_w_registers_sp_and_zero_register() {
    let program = image(&[
        0x92800000, // movn x0,#0 -> u64::MAX
        0x11000401, // add w1,w0,#1 -> 0, with W zero extension
        0x910023ff, // add sp,sp,#8
        0x910003e2, // add x2,sp,#0
        0x8b1f0003, // add x3,x0,xzr
        0xd28000ff, // movz xzr,#7 (discard)
    ]);
    let mut cpu = Cpu::new(program.entry, 100);
    for _ in 0..6 {
        step(&mut cpu, &program).unwrap();
    }
    assert_eq!(cpu.x[0], u64::MAX);
    assert_eq!(cpu.x[1], 0);
    assert_eq!(cpu.sp, 8);
    assert_eq!(cpu.x[2], 8);
    assert_eq!(cpu.x[3], u64::MAX);
}

#[test]
fn malformed_inputs_do_not_panic() {
    let original = include_bytes!("../fixtures/bin/hello.macho");
    for length in 0..original.len() {
        assert!(Image::parse(&original[..length]).is_err());
    }
    // Corrupt every load command size independently.
    let count = u32::from_le_bytes(original[16..20].try_into().unwrap());
    let mut offset = 32;
    for _ in 0..count {
        let mut bytes = original.to_vec();
        bytes[offset + 4..offset + 8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Image::parse(&bytes).is_err());
        offset += u32::from_le_bytes(original[offset + 4..offset + 8].try_into().unwrap()) as usize;
    }
}

#[test]
fn loader_rejects_wrong_arch_platform_permissions_and_entry() {
    let original = include_bytes!("../fixtures/bin/hello.macho");
    for (at, value) in [(0, 0), (4, 0x01000007), (8, 2), (12, 6), (16, u32::MAX)] {
        let mut b = original.to_vec();
        b[at..at + 4].copy_from_slice(&u32::to_le_bytes(value));
        assert!(Image::parse(&b).is_err());
    }
    let mut at = 32;
    for _ in 0..u32::from_le_bytes(original[16..20].try_into().unwrap()) {
        let cmd = u32::from_le_bytes(original[at..at + 4].try_into().unwrap());
        if cmd == 0x32 {
            let mut b = original.to_vec();
            b[at + 8..at + 12].copy_from_slice(&2u32.to_le_bytes());
            assert!(Image::parse(&b).is_err());
        }
        if cmd == 0x19 && &original[at + 8..at + 14] == b"__TEXT" {
            let mut b = original.to_vec();
            b[at + 60..at + 64].copy_from_slice(&7u32.to_le_bytes());
            assert!(Image::parse(&b).is_err());
            let mut b = original.to_vec();
            b[at + 32..at + 40].copy_from_slice(&u64::MAX.to_le_bytes());
            assert!(Image::parse(&b).is_err());
        }
        if cmd == 0x80000028 {
            let mut b = original.to_vec();
            b[at + 8..at + 16].copy_from_slice(&0u64.to_le_bytes());
            assert!(Image::parse(&b).is_err());
        }
        at += u32::from_le_bytes(original[at + 4..at + 8].try_into().unwrap()) as usize;
    }
}

#[test]
fn memory_permissions_and_bounds() {
    let program = image(&[0]);
    assert!(program.read(0x1000, 4, 1).is_ok());
    assert!(program.read(0x1000, 4, 2).is_err());
    assert!(program.read(0x1000, usize::MAX, 1).is_err());
    assert!(program.read(u64::MAX, 4, 1).is_err());
    assert!(program.instruction(0x1001).is_err());
}

#[test]
fn finite_budget_and_unknown_trap_fail() {
    let looping = image(&[0x14000000]);
    assert!(run(&looping, 10, false).unwrap_err().contains("budget"));
    let program = image(&[movz(16, 999), 0xd4001001]);
    assert!(run(&program, 10, false).unwrap_err().contains("999"));
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        assert!(run(&looping, 10, true).unwrap_err().contains("budget"));
        assert!(run(&program, 10, true).unwrap_err().contains("999"));
    }
}

#[test]
fn loader_rejects_unimplemented_optional_commands() {
    let original = include_bytes!("../fixtures/bin/hello.macho");
    // Replacing the first command is enough to check fail-closed handling.
    for command in [0x20u32, 0x3a, 0x10, 0x1234] {
        let mut bytes = original.to_vec();
        bytes[32..36].copy_from_slice(&command.to_le_bytes());
        assert!(Image::parse(&bytes).is_err());
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn exhausted_budget_precedes_faults_in_both_engines() {
    for words in [
        vec![0xffffffff],
        vec![0x14000100],
        vec![0xd65f03c0],
        vec![0xd503201f],
    ] {
        let program = image(&words);
        for budget in 0..=1 {
            let reference = run(&program, budget, false).unwrap_err();
            let translated = run(&program, budget, true).unwrap_err();
            assert_eq!(
                reference.contains("budget"),
                translated.contains("budget"),
                "{words:x?}, budget={budget}: {reference} / {translated}"
            );
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn differential_generated_programs_and_control_flow() {
    let mut seed = 0x716a5b8c4d3e2f01u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed as u32
    };
    for _ in 0..1000 {
        let mut words = Vec::new();
        for _ in 0..64 {
            let n = next();
            let dst = next() % 32;
            let src = next() % 32;
            let rhs = next() % 32;
            let width = (n & 1) << 31;
            words.push(match (n >> 1) % 8 {
                0 => 0x52800000 | width | (n & 0xffff) << 5 | dst,
                1 => 0x72800000 | width | (n & 0xffff) << 5 | dst,
                2 => 0x12800000 | width | (n & 0xffff) << 5 | dst,
                3 => 0x11000000 | width | (n & 4095) << 10 | src << 5 | dst,
                4 => 0x51000000 | width | (n & 4095) << 10 | src << 5 | dst,
                5 => 0x0b000000 | width | rhs << 16 | src << 5 | dst,
                6 => 0x4b000000 | width | rhs << 16 | src << 5 | dst,
                _ => 0x2a000000 | width | rhs << 16 | src << 5 | dst,
            });
        }
        words.extend(exit());
        let program = image(&words);
        assert_eq!(
            run(&program, 1000, true).unwrap(),
            run(&program, 1000, false).unwrap()
        );
    }
    // BL -> function at +20 -> RET -> B past function -> exit.
    let mut words = vec![
        movz(9, 7),
        0x94000004,
        0x14000005,
        0xd503201f,
        0xd503201f,
        0x91000529,
        0xd65f03c0,
    ];
    words.extend(exit());
    let program = image(&words);
    let reference = run(&program, 100, false).unwrap();
    assert_eq!(reference.cpu.x[9], 8);
    assert_eq!(run(&program, 100, true).unwrap(), reference);
    for words in [vec![0xffffffff], vec![0x14000100], vec![0xd65f03c0]] {
        let program = image(&words);
        assert!(run(&program, 10, false).is_err());
        assert!(run(&program, 10, true).is_err());
    }
}
