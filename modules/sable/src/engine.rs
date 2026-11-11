use crate::{
    Result,
    isa::{Op, decode},
    macho::Image,
    memory::{Memory, STACK_BASE, STACK_SIZE},
};
use std::time::Instant;

#[repr(C)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cpu {
    pub x: [u64; 31],
    pub sp: u64,
    pub pc: u64,
    pub remaining: u64,
    pub nzcv: u64,
}
impl Cpu {
    pub fn new(entry: u64, budget: u64) -> Self {
        Self {
            x: [0; 31],
            sp: 0,
            pc: entry,
            remaining: budget,
            nzcv: 0,
        }
    }
    pub fn reg(&self, n: u8, sp: bool) -> u64 {
        if n == 31 {
            if sp { self.sp } else { 0 }
        } else {
            self.x[n as usize]
        }
    }
    fn set(&mut self, n: u8, value: u64, wide: bool, sp: bool) {
        let value = if wide { value } else { value as u32 as u64 };
        if n != 31 {
            self.x[n as usize] = value;
        } else if sp {
            self.sp = value;
        }
    }
}

pub fn condition_holds(nzcv: u64, condition: u8) -> bool {
    let n = nzcv & (1 << 31) != 0;
    let z = nzcv & (1 << 30) != 0;
    let c = nzcv & (1 << 29) != 0;
    let v = nzcv & (1 << 28) != 0;
    let base = match condition >> 1 {
        0 => z,
        1 => c,
        2 => n,
        3 => v,
        4 => c && !z,
        5 => n == v,
        6 => n == v && !z,
        _ => true,
    };
    if condition & 1 != 0 && condition != 15 {
        !base
    } else {
        base
    }
}

pub fn shifted(value: u64, shift: u8, amount: u8, wide: bool) -> u64 {
    let value = if wide { value } else { value as u32 as u64 };
    match (shift, wide) {
        (0, _) => value << amount,
        (1, _) => value >> amount,
        (2, true) => ((value as i64) >> amount) as u64,
        (2, false) => ((value as i32) >> amount) as u32 as u64,
        (_, true) => value.rotate_right(u32::from(amount)),
        (_, false) => (value as u32).rotate_right(u32::from(amount)) as u64,
    }
}

fn arithmetic(cpu: &mut Cpu, a: u64, b: u64, subtract: bool, wide: bool, flags: bool) -> u64 {
    let mask = if wide { u64::MAX } else { u32::MAX as u64 };
    let a = a & mask;
    let b = b & mask;
    let result = if subtract {
        a.wrapping_sub(b)
    } else {
        a.wrapping_add(b)
    } & mask;
    if flags {
        let sign = 1u64 << if wide { 63 } else { 31 };
        let carry = if subtract {
            a >= b
        } else {
            (a as u128 + b as u128) > mask as u128
        };
        let overflow = ((if subtract { a ^ b } else { !(a ^ b) }) & (a ^ result) & sign) != 0;
        cpu.nzcv = ((result & sign != 0) as u64) << 31
            | ((result == 0) as u64) << 30
            | (carry as u64) << 29
            | (overflow as u64) << 28;
    }
    result
}

pub fn step(cpu: &mut Cpu, image: &Image) -> Result<bool> {
    transition(cpu, image, None)
}
pub fn step_memory(cpu: &mut Cpu, image: &Image, memory: &mut Memory) -> Result<bool> {
    transition(cpu, image, Some(memory))
}
fn transition(cpu: &mut Cpu, image: &Image, mut memory: Option<&mut Memory>) -> Result<bool> {
    if cpu.remaining == 0 {
        return Err("instruction budget exhausted".into());
    }
    let op = decode(image.instruction(cpu.pc)?, cpu.pc)?;
    let mut next = cpu.pc + 4;
    let mut trapped = false;
    match op {
        Op::Indirect { reg, link } => {
            let target = cpu.reg(reg, false);
            if link {
                cpu.x[30] = next;
            }
            next = target;
        }
        Op::Literal {
            reg,
            address,
            bytes,
        } => {
            let memory = memory
                .as_mut()
                .ok_or("literal load requires per-run memory")?;
            let mut value = [0; 8];
            value[..bytes as usize].copy_from_slice(memory.read(address, bytes as usize)?);
            cpu.set(reg, u64::from_le_bytes(value), true, false);
        }
        Op::MemoryReg {
            reg,
            base,
            index,
            option,
            scaled,
            bytes,
            load,
        } => {
            let memory = memory
                .as_mut()
                .ok_or("memory operation requires per-run memory")?;
            let raw = cpu.reg(index, false);
            let offset = match option {
                2 => raw as u32 as u64,
                6 => raw as i32 as i64 as u64,
                _ => raw,
            };
            let offset = offset.wrapping_shl(if scaled { bytes.trailing_zeros() } else { 0 });
            let original = cpu.reg(base, true);
            if base == 31 && !original.is_multiple_of(16) {
                return Err("unaligned guest SP".into());
            }
            let address = original.wrapping_add(offset);
            if load {
                let mut value = [0; 8];
                value[..bytes as usize].copy_from_slice(memory.read(address, bytes as usize)?);
                cpu.set(reg, u64::from_le_bytes(value), true, false);
            } else {
                memory.write(
                    address,
                    &cpu.reg(reg, false).to_le_bytes()[..bytes as usize],
                )?;
            }
        }
        Op::MovWide {
            dst,
            value,
            keep,
            shift,
            wide,
        } => {
            let value = if keep {
                (cpu.reg(dst, false) & !(0xffffu64 << shift)) | value
            } else {
                value
            };
            cpu.set(dst, value, wide, false);
        }
        Op::AddImm {
            dst,
            src,
            value,
            subtract,
            wide,
            flags,
        } => {
            let result = arithmetic(cpu, cpu.reg(src, true), value, subtract, wide, flags);
            cpu.set(dst, result, wide, !flags);
        }
        Op::AddReg {
            dst,
            lhs,
            rhs,
            subtract,
            wide,
            flags,
            shift,
            amount,
        } => {
            let b = shifted(cpu.reg(rhs, false), shift, amount, wide);
            let result = arithmetic(cpu, cpu.reg(lhs, false), b, subtract, wide, flags);
            cpu.set(dst, result, wide, false);
        }
        Op::OrReg {
            dst,
            lhs,
            rhs,
            wide,
        } => cpu.set(dst, cpu.reg(lhs, false) | cpu.reg(rhs, false), wide, false),
        Op::Logical {
            dst,
            lhs,
            rhs,
            kind,
            invert,
            shift,
            amount,
            wide,
        } => {
            let a = cpu.reg(lhs, false);
            let b = shifted(cpu.reg(rhs, false), shift, amount, wide);
            let b = if invert { !b } else { b };
            let result = match kind {
                0 | 3 => a & b,
                1 => a | b,
                _ => a ^ b,
            };
            if kind == 3 {
                let r = if wide { result } else { result as u32 as u64 };
                cpu.nzcv = ((r >> if wide { 63 } else { 31 }) & 1) << 31 | ((r == 0) as u64) << 30;
            }
            cpu.set(dst, result, wide, false);
        }
        Op::LogicalImm {
            dst,
            src,
            value,
            kind,
            wide,
        } => {
            let a = cpu.reg(src, false);
            let result = match kind {
                0 | 3 => a & value,
                1 => a | value,
                _ => a ^ value,
            };
            if kind == 3 {
                let r = if wide { result } else { result as u32 as u64 };
                cpu.nzcv = ((r >> if wide { 63 } else { 31 }) & 1) << 31 | ((r == 0) as u64) << 30;
            }
            cpu.set(dst, result, wide, kind != 3);
        }
        Op::CondSelect {
            dst,
            lhs,
            rhs,
            condition,
            invert,
            increment,
            wide,
        } => {
            let result = if condition_holds(cpu.nzcv, condition) {
                cpu.reg(lhs, false)
            } else {
                let b = cpu.reg(rhs, false);
                (if invert { !b } else { b }).wrapping_add(u64::from(increment))
            };
            cpu.set(dst, result, wide, false);
        }
        Op::CondCompare {
            lhs,
            rhs,
            immediate,
            subtract,
            wide,
            condition,
            nzcv,
        } => {
            if condition_holds(cpu.nzcv, condition) {
                let b = if immediate {
                    u64::from(rhs)
                } else {
                    cpu.reg(rhs, false)
                };
                arithmetic(cpu, cpu.reg(lhs, false), b, subtract, wide, true);
            } else {
                cpu.nzcv = u64::from(nzcv) << 28;
            }
        }
        Op::CondBranch { target, condition } => {
            if condition_holds(cpu.nzcv, condition) {
                next = target;
            }
        }
        Op::Memory {
            reg,
            base,
            offset,
            bytes,
            load,
            writeback,
            post,
        } => {
            let memory = memory
                .as_mut()
                .ok_or("memory operation requires per-run memory")?;
            let original = cpu.reg(base, true);
            if base == 31 && !original.is_multiple_of(16) {
                return Err("unaligned guest SP".into());
            }
            let adjusted = original.wrapping_add_signed(offset);
            let address = if post { original } else { adjusted };
            if load {
                let mut value = [0; 8];
                value[..bytes as usize].copy_from_slice(memory.read(address, bytes as usize)?);
                cpu.set(reg, u64::from_le_bytes(value), true, false);
            } else {
                memory.write(
                    address,
                    &cpu.reg(reg, false).to_le_bytes()[..bytes as usize],
                )?;
            }
            if writeback {
                cpu.set(base, adjusted, true, true);
            }
        }
        Op::Pair {
            first,
            second,
            base,
            offset,
            bytes,
            load,
            writeback,
            post,
        } => {
            let memory = memory
                .as_mut()
                .ok_or("memory operation requires per-run memory")?;
            let original = cpu.reg(base, true);
            if base == 31 && !original.is_multiple_of(16) {
                return Err("unaligned guest SP".into());
            }
            let adjusted = original.wrapping_add_signed(offset);
            let address = if post { original } else { adjusted };
            let size = bytes as usize;
            if load {
                let data = memory.read(address, size * 2)?;
                let mut a = [0; 8];
                let mut b = [0; 8];
                a[..size].copy_from_slice(&data[..size]);
                b[..size].copy_from_slice(&data[size..]);
                cpu.set(first, u64::from_le_bytes(a), true, false);
                cpu.set(second, u64::from_le_bytes(b), true, false);
            } else {
                let mut data = [0; 16];
                data[..size].copy_from_slice(&cpu.reg(first, false).to_le_bytes()[..size]);
                data[size..size * 2].copy_from_slice(&cpu.reg(second, false).to_le_bytes()[..size]);
                memory.write(address, &data[..size * 2])?;
            }
            if writeback {
                cpu.set(base, adjusted, true, true);
            }
        }
        Op::Address { dst, value } => cpu.set(dst, value, true, false),
        Op::Branch { target, link } => {
            if link {
                cpu.x[30] = next;
            }
            next = target;
        }
        Op::CompareBranch {
            reg,
            target,
            nonzero,
            wide,
        } => {
            let value = cpu.reg(reg, false);
            if (if wide { value } else { value as u32 as u64 } != 0) == nonzero {
                next = target;
            }
        }
        Op::Return { reg } => next = cpu.reg(reg, false),
        Op::Svc => trapped = true,
        Op::Nop => {}
    }
    cpu.remaining -= 1;
    cpu.pc = next;
    Ok(trapped)
}

#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    pub cpu: Cpu,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit: u8,
    pub syscalls: u64,
    pub memory: Memory,
}

fn syscall(
    cpu: &mut Cpu,
    memory: &Memory,
    out: &mut [Vec<u8>; 2],
    started: Instant,
) -> Result<Option<u8>> {
    match cpu.x[16] as i64 {
        1 => Ok(Some(cpu.x[0] as u8)),
        4 => {
            let channel = match cpu.x[0] {
                1 => 0,
                2 => 1,
                fd => return Err(format!("unsupported guest write fd {fd}")),
            };
            let len = usize::try_from(cpu.x[2]).map_err(|_| "write size overflow")?;
            if len > 1024 * 1024 || out[channel].len().saturating_add(len) > 4 * 1024 * 1024 {
                return Err("guest output limit exceeded".into());
            }
            out[channel].extend_from_slice(memory.read(cpu.x[1], len)?);
            cpu.x[0] = len as u64;
            cpu.nzcv &= !(1 << 29);
            Ok(None)
        }
        20 => {
            cpu.x[0] = 1;
            cpu.nzcv &= !(1 << 29);
            Ok(None)
        }
        -3 => {
            cpu.x[0] = started.elapsed().as_nanos() as u64;
            Ok(None)
        }
        n => Err(format!(
            "unimplemented Darwin syscall/trap {n} at {:#x}",
            cpu.pc - 4
        )),
    }
}

/// Immutable translated code may be reused across independent, freshly reset runs.
const CALL_RETURN: u64 = u64::MAX - 3;

fn validate_startup(image: &Image) -> Result<()> {
    crate::tls::layout(image)?;
    if let Some(plan) = &image.objc {
        plan.validate(image)?;
    }
    if let Some(startup) = &image.startup {
        image.read(startup.header, 32, 1)?;
        if image.contains_pc(CALL_RETURN)
            || startup.initializers.len() > 1024
            || startup
                .initializers
                .iter()
                .any(|&pc| !image.contains_pc(pc))
        {
            return Err("invalid prepared initializer plan".into());
        }
    }
    Ok(())
}

struct Process {
    cpu: Cpu,
    memory: Memory,
    out: [Vec<u8>; 2],
    syscalls: u64,
    started: Instant,
}

struct Arguments {
    sp: u64,
    argv: u64,
    envp: u64,
    apple: u64,
    vars: u64,
}
impl Process {
    fn arguments(&mut self, header: u64) -> Result<Arguments> {
        let base = STACK_BASE + STACK_SIZE as u64 - 4096;
        let args = Arguments {
            sp: base,
            argv: base + 0x100,
            envp: base + 0x120,
            apple: base + 0x130,
            vars: base + 0x140,
        };
        self.memory.write(base + 0x200, b"sable\0")?;
        self.memory
            .write(base + 0x220, b"executable_path=sable\0")?;
        self.memory.write(base + 0x180, &1u32.to_le_bytes())?;
        for (address, values) in [
            (args.argv, vec![base + 0x200, 0]),
            (args.envp, vec![0]),
            (args.apple, vec![base + 0x220, 0]),
            (base + 0x188, vec![args.argv]),
            (base + 0x190, vec![args.envp]),
            (base + 0x198, vec![base + 0x200]),
            (
                args.vars,
                vec![
                    header,
                    base + 0x180,
                    base + 0x188,
                    base + 0x190,
                    base + 0x198,
                ],
            ),
        ] {
            for (index, value) in values.into_iter().enumerate() {
                self.memory
                    .write(address + index as u64 * 8, &value.to_le_bytes())?;
            }
        }
        Ok(args)
    }
    fn call(&mut self, pc: u64, args: &Arguments) {
        self.cpu = Cpu::new(pc, self.cpu.remaining);
        self.cpu.sp = args.sp;
        self.cpu.x[..5].copy_from_slice(&[1, args.argv, args.envp, args.apple, args.vars]);
        self.cpu.x[30] = CALL_RETURN;
    }
    fn finish(self, exit: u8) -> Outcome {
        let [stdout, stderr] = self.out;
        Outcome {
            cpu: self.cpu,
            memory: self.memory,
            stdout,
            stderr,
            exit,
            syscalls: self.syscalls,
        }
    }
}

pub struct Prepared<'a> {
    image: &'a Image,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    compiled: Option<crate::jit::Compiled>,
}
impl<'a> Prepared<'a> {
    pub fn new(image: &'a Image, jit: bool) -> Result<Self> {
        validate_startup(image)?;
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let compiled = if jit {
            Some(crate::jit::Compiled::new(image)?)
        } else {
            None
        };
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        if jit {
            return Err("native translation currently requires x86_64 Linux".into());
        }
        Ok(Self {
            image,
            #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
            compiled,
        })
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    pub fn new_with_dispatch(image: &'a Image, native_indirect: bool) -> Result<Self> {
        validate_startup(image)?;
        Ok(Self {
            image,
            compiled: Some(crate::jit::Compiled::new_with_dispatch(
                image,
                native_indirect,
            )?),
        })
    }
    pub fn run(&self, budget: u64) -> Result<Outcome> {
        let mut cpu = Cpu::new(self.image.entry, budget);
        cpu.sp = STACK_BASE + STACK_SIZE as u64;
        let mut process = Process {
            cpu,
            memory: Memory::new(self.image)?,
            out: [Vec::new(), Vec::new()],
            syscalls: 0,
            started: Instant::now(),
        };
        if let Some(startup) = &self.image.startup {
            let args = process.arguments(startup.header)?;
            for &target in &startup.initializers {
                process.call(target, &args);
                if let Some(exit) = self
                    .drive(&mut process, Some(CALL_RETURN))
                    .map_err(|e| format!("initializer {target:#x}: {e}"))?
                {
                    return Ok(process.finish(exit));
                }
                if process.cpu.sp != args.sp {
                    return Err("initializer returned with unbalanced SP".into());
                }
            }
            process.call(self.image.entry, &args);
            let exit = self
                .drive(&mut process, Some(CALL_RETURN))?
                .unwrap_or(process.cpu.x[0] as u8);
            if process.cpu.pc == CALL_RETURN && process.cpu.sp != args.sp {
                return Err("main returned with unbalanced SP".into());
            }
            return Ok(process.finish(exit));
        }
        let exit = self
            .drive(&mut process, None)?
            .ok_or("unexpected freestanding return")?;
        Ok(process.finish(exit))
    }

    fn drive(&self, process: &mut Process, return_pc: Option<u64>) -> Result<Option<u8>> {
        let Process {
            cpu,
            memory,
            out,
            syscalls,
            started,
        } = process;
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let compiled = &self.compiled;
        loop {
            if return_pc == Some(cpu.pc) {
                return Ok(None);
            }
            #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
            let trapped = if let Some(code) = &compiled {
                code.execute_until(cpu, self.image, memory, return_pc)?
            } else {
                step_memory(cpu, self.image, memory)?
            };
            #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
            let trapped = step_memory(cpu, self.image, memory)?;
            if trapped {
                let trap = cpu.pc.checked_sub(4).ok_or("invalid trap PC")?;
                if self.image.instruction(trap)? == crate::tls::SVC {
                    cpu.x[0] = memory.resolve_tlv(self.image, cpu.x[0], trap)?;
                    continue;
                }
                let word = self.image.instruction(trap)?;
                if word == crate::objc::GET_CLASS || word == crate::objc::LOOKUP {
                    let plan = self.image.objc.as_ref().ok_or("unregistered ObjC trap")?;
                    let result = plan.trap(memory, trap, word, cpu.x[0], cpu.x[1])?;
                    if word == crate::objc::GET_CLASS {
                        cpu.x[0] = result;
                    } else {
                        cpu.x[16] = result;
                    }
                    continue;
                }
                *syscalls += 1;
                if let Some(exit) = syscall(cpu, memory, out, *started)? {
                    return Ok(Some(exit));
                }
            }
        }
    }
}

pub fn run(image: &Image, budget: u64, jit: bool) -> Result<Outcome> {
    Prepared::new(image, jit)?.run(budget)
}
