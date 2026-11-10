//! Test-only binary protocol. No oracle dependency is linked into Sable.
use sable::{
    engine::{Cpu, step_memory},
    macho::{Image, Segment},
    memory::{Memory, STACK_BASE},
};
use std::io::{self, Read};

fn take<'a>(input: &mut &'a [u8], size: usize) -> &'a [u8] {
    let (head, tail) = input
        .split_at_checked(size)
        .expect("truncated oracle input");
    *input = tail;
    head
}
fn u32in(input: &mut &[u8]) -> u32 {
    u32::from_le_bytes(take(input, 4).try_into().unwrap())
}
fn u64in(input: &mut &[u8]) -> u64 {
    u64::from_le_bytes(take(input, 8).try_into().unwrap())
}
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(s, "{b:02x}").unwrap();
    }
    s
}

fn main() {
    let jit = std::env::args().nth(1).as_deref() == Some("jit");
    let mut input = Vec::new();
    io::stdin()
        .take(128 * 1024 * 1024)
        .read_to_end(&mut input)
        .unwrap();
    let mut input = input.as_slice();
    assert_eq!(take(&mut input, 8), b"SABLE001");
    let cases = u32in(&mut input);
    assert!(cases <= 10000);
    for id in 0..cases {
        let count = u32in(&mut input) as usize;
        assert!((1..=4096).contains(&count));
        let mut cpu = Cpu::new(0x1000, 10000);
        for x in &mut cpu.x {
            *x = u64in(&mut input);
        }
        cpu.sp = u64in(&mut input);
        cpu.nzcv = u64in(&mut input);
        let code = take(&mut input, count * 4).to_vec();
        let data = take(&mut input, 512).to_vec();
        let stack = take(&mut input, 4096);
        let image = Image {
            segments: vec![
                Segment {
                    address: 0x1000,
                    bytes: code.into(),
                    protection: 5,
                },
                Segment {
                    address: 0x20000,
                    bytes: data.into(),
                    protection: 3,
                },
            ],
            entry: 0x1000,
            text: (0x1000, (count * 4) as u64),
            extra_text: Vec::new(),
            platform: 7,
        };
        let mut memory = Memory::new(&image).unwrap();
        memory.write(STACK_BASE, stack).unwrap();
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let result = if jit {
            sable::jit::Compiled::new(&image)
                .and_then(|c| c.execute_memory(&mut cpu, &image, &mut memory))
        } else {
            until_svc(&mut cpu, &image, &mut memory)
        };
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        let result = if jit {
            Err("JIT requires x86_64 Linux".into())
        } else {
            until_svc(&mut cpu, &image, &mut memory)
        };
        match result {
            Ok(()) => println!(
                "{{\"id\":{id},\"x\":{:?},\"sp\":{},\"nzcv\":{},\"pc\":{},\"instructions\":{},\"data\":\"{}\",\"stack\":\"{}\"}}",
                cpu.x,
                cpu.sp,
                cpu.nzcv,
                cpu.pc,
                10000 - cpu.remaining,
                hex(memory.read(0x20000, 512).unwrap()),
                hex(memory.read(STACK_BASE, 4096).unwrap())
            ),
            Err(e) => panic!("case {id}: {e}"),
        }
    }
    assert!(input.is_empty());
}
fn until_svc(cpu: &mut Cpu, image: &Image, memory: &mut Memory) -> sable::Result<()> {
    while !step_memory(cpu, image, memory)? {}
    Ok(())
}
