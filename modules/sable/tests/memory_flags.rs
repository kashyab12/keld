use sable::{
    engine::{Cpu, condition_holds, run, step_memory},
    isa::decode,
    macho::{Image, Segment},
    memory::{Memory, STACK_BASE},
};

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
    }
}

#[test]
fn compiled_c_checksums_and_private_memory() {
    let image = Image::parse(include_bytes!("../fixtures/bin/workload.macho")).unwrap();
    let expected = 0x8e65b49356b423fau64.to_le_bytes();
    let a = run(&image, 2_000_000, false).unwrap();
    let b = run(&image, 2_000_000, false).unwrap();
    assert_eq!(a.stdout, expected);
    assert_eq!(a, b);
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    assert_eq!(run(&image, 2_000_000, true).unwrap(), a);
}

#[test]
fn arithmetic_checksum_is_observable_by_external_runner() {
    let image = Image::parse(include_bytes!("../fixtures/bin/checksum.macho")).unwrap();
    let result = run(&image, 40_000_000, false).unwrap();
    assert_eq!(result.stdout, 50_000_005_000_000u64.to_le_bytes());
    assert_eq!(result.exit, 0);
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    assert_eq!(run(&image, 40_000_000, true).unwrap(), result);
}

#[test]
fn flags_have_arm_carry_and_signed_overflow_semantics() {
    for (word, a, b, expected) in [
        (0xab010002, u64::MAX, 1, 0x60000000),
        (0xeb010002, 0, 1, 0x80000000),
        (0xab010002, i64::MAX as u64, 1, 0x90000000),
        (0x2b010002, 0x7fffffff, 1, 0x90000000),
        (0x6b010002, 0, 0, 0x60000000),
    ] {
        let image = program(&[word]);
        let mut memory = Memory::new(&image).unwrap();
        let mut cpu = Cpu::new(image.entry, 10);
        cpu.x[0] = a;
        cpu.x[1] = b;
        step_memory(&mut cpu, &image, &mut memory).unwrap();
        assert_eq!(cpu.nzcv, expected);
    }
    assert!(condition_holds(0, 14));
    assert!(condition_holds(0, 15));
}

#[test]
fn faults_preserve_cpu_and_memory_before_writeback() {
    // STR X0,[X1],#8: invalid address must not write X1 or consume an instruction.
    // STR X0,[X1]: cross-region, readonly, overflow and wholly unmapped accesses.
    for word in [0xf8008420, 0xf9000020] {
        for address in [0x20000 + 60, 0x1000, u64::MAX - 3, 0] {
            let image = program(&[word]);
            let mut memory = Memory::new(&image).unwrap();
            let mut cpu = Cpu::new(image.entry, 10);
            cpu.x[0] = 123;
            cpu.x[1] = address;
            let before = cpu.clone();
            let bytes = memory.read(0x20000, 64).unwrap().to_vec();
            assert!(step_memory(&mut cpu, &image, &mut memory).is_err());
            assert_eq!(cpu, before);
            assert_eq!(memory.read(0x20000, 64).unwrap(), bytes);
            #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
            {
                let code = sable::jit::Compiled::new(&image).unwrap();
                assert!(code.execute_memory(&mut cpu, &image, &mut memory).is_err());
                assert_eq!(cpu, before);
                assert_eq!(memory.read(0x20000, 64).unwrap(), bytes);
            }
        }
    }
}

#[test]
fn pair_fault_does_not_partially_store_or_update_base() {
    let image = program(&[0xa9810820]); // STP X0,X2,[X1,#16]!
    let mut memory = Memory::new(&image).unwrap();
    let mut cpu = Cpu::new(image.entry, 10);
    cpu.x[0] = 123;
    cpu.x[1] = 0x20000 + 40;
    cpu.x[2] = 456;
    let before = cpu.clone();
    assert!(step_memory(&mut cpu, &image, &mut memory).is_err());
    assert_eq!(cpu, before);
    assert_eq!(memory.read(0x20000, 64).unwrap(), [0; 64]);
}

#[test]
fn reject_vector_signed_unprivileged_and_overlap_encodings() {
    for word in [
        0x3dc00000, 0x39800000, 0xb8400800, 0xf8408400, 0xa9bf0000, 0xa9400000,
    ] {
        assert!(decode(word, 0x1000).is_err(), "{word:x}");
    }
}

#[test]
fn stack_alignment_and_region_collision() {
    let image = program(&[0xf90003e0]);
    let mut memory = Memory::new(&image).unwrap();
    let mut cpu = Cpu::new(image.entry, 10);
    cpu.sp = STACK_BASE + 1;
    assert!(
        step_memory(&mut cpu, &image, &mut memory)
            .unwrap_err()
            .contains("SP")
    );
    let mut collision = image;
    collision.segments.push(Segment {
        address: STACK_BASE,
        bytes: vec![0; 4].into(),
        protection: 3,
    });
    assert!(Memory::new(&collision).is_err());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_block_budgets_preserve_partial_state_and_mid_block_entries() {
    for words in [
        vec![0x91000400, 0x91000821, 0xeb010002, 0xd4001001],
        vec![0x91000400, 0x91000821, 0x17fffffe],
        vec![0x91000400, 0xd65f03c0],
    ] {
        let image = program(&words);
        let code = sable::jit::Compiled::new(&image).unwrap();
        for entry in (image.text.0..image.text.0 + image.text.1).step_by(4) {
            for budget in 0..=12 {
                let mut reference = Cpu::new(entry, budget);
                reference.x[30] = image.text.0 + 4;
                reference.x[0] = u64::MAX;
                reference.x[1] = 1;
                let mut translated = reference.clone();
                let mut a = Memory::new(&image).unwrap();
                let mut b = Memory::new(&image).unwrap();
                let expected = loop {
                    match step_memory(&mut reference, &image, &mut a) {
                        Ok(false) => {}
                        Ok(true) => break Ok(()),
                        Err(e) => break Err(e),
                    }
                };
                let observed = code.execute_memory(&mut translated, &image, &mut b);
                assert_eq!(expected.is_ok(), observed.is_ok());
                assert_eq!(
                    reference, translated,
                    "entry={entry:x},budget={budget},words={words:x?}"
                );
                assert_eq!(a, b);
            }
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn indirect_dispatch_checks_ranges_alignment_and_budget_before_jump() {
    for native in [false, true] {
        let mut image = program(&[0xd61f0120, 0xd4001001]);
        image.extra_text.push((0x3000, 8));
        image.segments.push(Segment {
            address: 0x3000,
            bytes: [0x91000400u32, 0xd65f03c0]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>()
                .into(),
            protection: 5,
        });
        let code = sable::jit::Compiled::new_with_dispatch(&image, native).unwrap();
        for target in [0x1004, 0x3000, 0x3004, 0x3001, 0x20000, 0, u64::MAX] {
            for budget in 0..=6 {
                let mut a = Cpu::new(image.entry, budget);
                a.x[9] = target;
                a.x[30] = 0x1004;
                let mut b = a.clone();
                let mut x = Memory::new(&image).unwrap();
                let mut y = Memory::new(&image).unwrap();
                let expected = loop {
                    match step_memory(&mut a, &image, &mut x) {
                        Ok(false) => {}
                        Ok(true) => break Ok(()),
                        Err(e) => break Err(e),
                    }
                };
                let actual = code.execute_memory(&mut b, &image, &mut y);
                assert_eq!(expected.is_ok(), actual.is_ok());
                assert_eq!(a, b, "target={target:x},budget={budget},native={native}");
                assert_eq!(x, y);
            }
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn discontiguous_images_do_not_allow_fallthrough_across_gaps() {
    let mut image = program(&[0xd503201f]);
    image.extra_text.push((0x3000, 4));
    image.segments.push(Segment {
        address: 0x3000,
        bytes: 0xd4001001u32.to_le_bytes().to_vec().into(),
        protection: 5,
    });
    let code = sable::jit::Compiled::new(&image).unwrap();
    let mut cpu = Cpu::new(image.entry, 10);
    let mut memory = Memory::new(&image).unwrap();
    assert!(code.execute_memory(&mut cpu, &image, &mut memory).is_err());
    assert_eq!(cpu.pc, 0x1004);
    assert_eq!(cpu.remaining, 9);
}
