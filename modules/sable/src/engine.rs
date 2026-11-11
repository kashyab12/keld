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
    /// Only FPCR = 0 is representable: RN rounding, FZ = 0, DN = 0, no traps.
    pub fpcr: u64,
    /// Cumulative IOC/DZC/OFC/UFC/IXC/IDC and QC bits (`fp::FPSR_MASK`).
    pub fpsr: u64,
    /// V0-V31; scalar B/H/S/D writes zero the upper bits.
    pub v: [u128; 32],
}
impl Cpu {
    pub fn new(entry: u64, budget: u64) -> Self {
        Self {
            x: [0; 31],
            sp: 0,
            pc: entry,
            remaining: budget,
            nzcv: 0,
            fpcr: 0,
            fpsr: 0,
            v: [0; 32],
        }
    }
    /// Write Xn (XZR discards).
    pub fn set_x(&mut self, n: u8, value: u64) {
        self.set(n, value, true, false);
    }
    /// Write Xn, or SP for register 31 (base-register writeback).
    pub fn set_sp_or_x(&mut self, n: u8, value: u64) {
        self.set(n, value, true, true);
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
            signed,
        } => {
            let memory = memory
                .as_mut()
                .ok_or("literal load requires per-run memory")?;
            let mut value = [0; 8];
            value[..bytes as usize].copy_from_slice(memory.read(address, bytes as usize)?);
            let value = u64::from_le_bytes(value);
            cpu.set(
                reg,
                if signed {
                    value as i32 as i64 as u64
                } else {
                    value
                },
                true,
                false,
            );
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
        Op::UnsignedBitfield {
            dst,
            src,
            right,
            left,
            mask,
            wide,
        } => {
            let value = cpu.reg(src, false);
            let value = if wide { value } else { value as u32 as u64 };
            cpu.set(dst, ((value >> right) & mask) << left, wide, false);
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
        Op::TestBranch {
            reg,
            bit,
            target,
            nonzero,
        } => {
            if ((cpu.reg(reg, false) & (1u64 << bit)) != 0) == nonzero {
                next = target;
            }
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
        Op::Fp(op) => crate::fp::execute(cpu, op, memory, cpu.pc)?,
        Op::DataProc2 {
            dst,
            lhs,
            rhs,
            kind,
            wide,
        } => {
            let width = if wide { 64 } else { 32 };
            let mask = if wide { u64::MAX } else { u32::MAX as u64 };
            let a = cpu.reg(lhs, false) & mask;
            let b = cpu.reg(rhs, false) & mask;
            let amount = (b % width) as u32;
            let result = match kind {
                2 => a.checked_div(b).unwrap_or(0),
                3 => {
                    let (x, y) = if wide {
                        (a as i64, b as i64)
                    } else {
                        (a as u32 as i32 as i64, b as u32 as i32 as i64)
                    };
                    if y == 0 {
                        0
                    } else if wide {
                        x.wrapping_div(y) as u64
                    } else {
                        (x as i32).wrapping_div(y as i32) as u32 as u64
                    }
                }
                8 => a << amount,
                9 => a >> amount,
                10 => {
                    if wide {
                        ((a as i64) >> amount) as u64
                    } else {
                        ((a as u32 as i32) >> amount) as u32 as u64
                    }
                }
                _ => {
                    if wide {
                        a.rotate_right(amount)
                    } else {
                        (a as u32).rotate_right(amount) as u64
                    }
                }
            };
            cpu.set(dst, result, wide, false);
        }
        Op::DataProc1 {
            dst,
            src,
            kind,
            wide,
        } => {
            let a = cpu.reg(src, false);
            let w = a as u32;
            let result = match (kind, wide) {
                (0, true) => a.reverse_bits(),
                (0, false) => w.reverse_bits() as u64,
                (1, _) => {
                    let swapped =
                        ((a & 0x00ff_00ff_00ff_00ff) << 8) | ((a >> 8) & 0x00ff_00ff_00ff_00ff);
                    if wide { swapped } else { swapped as u32 as u64 }
                }
                (2, true) => {
                    ((a as u32).swap_bytes() as u64)
                        | (((a >> 32) as u32).swap_bytes() as u64) << 32
                }
                (2, false) => w.swap_bytes() as u64,
                (3, _) => a.swap_bytes(),
                (4, true) => u64::from(a.leading_zeros()),
                (4, false) => u64::from(w.leading_zeros()),
                (_, true) => u64::from((a ^ (a << 1) | 1).leading_zeros()),
                (_, false) => u64::from((w ^ (w << 1) | 1).leading_zeros()),
            };
            cpu.set(dst, result, wide, false);
        }
        Op::MulAdd {
            dst,
            lhs,
            rhs,
            acc,
            op31,
            subtract,
            wide,
        } => {
            let a = cpu.reg(lhs, false);
            let b = cpu.reg(rhs, false);
            let c = cpu.reg(acc, false);
            let product = match op31 {
                0 => a.wrapping_mul(b),
                1 => (a as u32 as i32 as i64).wrapping_mul(b as u32 as i32 as i64) as u64,
                5 => (a as u32 as u64).wrapping_mul(b as u32 as u64),
                2 => ((i128::from(a as i64) * i128::from(b as i64)) >> 64) as u64,
                _ => ((u128::from(a) * u128::from(b)) >> 64) as u64,
            };
            let result = match op31 {
                2 | 6 => product,
                _ if subtract => c.wrapping_sub(product),
                _ => c.wrapping_add(product),
            };
            cpu.set(dst, result, wide, false);
        }
        Op::SignedBitfield {
            dst,
            src,
            right,
            left,
            bits,
            wide,
        } => {
            let value = cpu.reg(src, false);
            let value = if wide { value } else { value as u32 as u64 };
            let field = (value >> right) & (u64::MAX >> (64 - bits));
            let shift = 64 - u32::from(bits);
            let extended = (((field << shift) as i64) >> shift) as u64;
            cpu.set(dst, extended << left, wide, false);
        }
        Op::Extract {
            dst,
            lhs,
            rhs,
            lsb,
            wide,
        } => {
            let high = cpu.reg(lhs, false);
            let low = cpu.reg(rhs, false);
            let result = if wide {
                ((u128::from(high) << 64 | u128::from(low)) >> lsb) as u64
            } else {
                ((u64::from(high as u32) << 32 | u64::from(low as u32)) >> lsb) as u32 as u64
            };
            cpu.set(dst, result, wide, false);
        }
        Op::SignedLoad {
            reg,
            base,
            offset,
            index,
            bytes,
            wide,
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
            let offset = match index {
                Some((index, option, scaled)) => {
                    let raw = cpu.reg(index, false);
                    let extended = match option {
                        2 => raw as u32 as u64,
                        6 => raw as i32 as i64 as u64,
                        _ => raw,
                    };
                    extended.wrapping_shl(if scaled { bytes.trailing_zeros() } else { 0 }) as i64
                }
                None => offset,
            };
            let adjusted = original.wrapping_add_signed(offset);
            let address = if post { original } else { adjusted };
            let mut value = [0; 8];
            value[..bytes as usize].copy_from_slice(memory.read(address, bytes as usize)?);
            let shift = 64 - 8 * u32::from(bytes);
            let extended = ((u64::from_le_bytes(value) << shift) as i64 >> shift) as u64;
            cpu.set(reg, extended, wide, false);
            if writeback {
                cpu.set(base, adjusted, true, true);
            }
        }
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
    fn finish(self, exit: u8) -> Result<Outcome> {
        self.memory.ownership_complete()?;
        let [stdout, stderr] = self.out;
        Ok(Outcome {
            cpu: self.cpu,
            memory: self.memory,
            stdout,
            stderr,
            exit,
            syscalls: self.syscalls,
        })
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
                    return process.finish(exit);
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
            return process.finish(exit);
        }
        let exit = self
            .drive(&mut process, None)?
            .ok_or("unexpected freestanding return")?;
        process.finish(exit)
    }

    fn drive(&self, process: &mut Process, return_pc: Option<u64>) -> Result<Option<u8>> {
        let mut continuation: Option<(Cpu, u64)> = None;
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
            let active_return = if continuation.is_some() {
                Some(crate::objc::ownership::RETURN)
            } else {
                return_pc
            };
            if continuation.is_some() && active_return == Some(cpu.pc) {
                let (saved, object) = continuation.take().unwrap();
                if cpu.sp != saved.sp {
                    return Err("guest dealloc returned with unbalanced SP".into());
                }
                // AAPCS64: d8-d15 are callee-saved, so a dealloc that changes them is rejected.
                if let Some(n) = (8..16).find(|&n| cpu.v[n] as u64 != saved.v[n] as u64) {
                    return Err(format!(
                        "guest dealloc clobbered callee-saved d{n} across the release service"
                    ));
                }
                let plan = self.image.objc.as_ref().ok_or("ownership plan absent")?;
                plan.finish_dealloc(memory, object)?;
                // Native void ABI: restore callee-preserved state and the checked service resume.
                // The trapping caller sees v0-v31 and FPCR exactly as it left them.
                cpu.x[18..].copy_from_slice(&saved.x[18..]);
                cpu.v = saved.v;
                cpu.fpcr = saved.fpcr;
                cpu.sp = saved.sp;
                cpu.pc = saved.pc;
                cpu.x[0] = 0;
                if let crate::objc::ownership::Action::Dealloc {
                    object,
                    selector,
                    imp,
                } = plan.drain_next(memory)?
                {
                    continuation = Some((cpu.clone(), object));
                    cpu.x[0] = object;
                    cpu.x[1] = selector;
                    cpu.x[30] = crate::objc::ownership::RETURN;
                    cpu.pc = imp;
                }
                continue;
            }
            if active_return == Some(cpu.pc) {
                return Ok(None);
            }
            #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
            let trapped = if let Some(code) = &compiled {
                code.execute_until(cpu, self.image, memory, active_return)?
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
                if crate::objc::ownership::is_service(word) {
                    if continuation.is_some() {
                        return Err("lifecycle reentrancy unsupported".into());
                    }
                    let plan = self
                        .image
                        .objc
                        .as_ref()
                        .ok_or("unregistered ownership trap")?;
                    match plan.ownership_trap(
                        memory,
                        trap,
                        word,
                        cpu.x[0],
                        crate::objc::ownership::release_register(word).map_or(0, |r| cpu.x[r]),
                    )? {
                        crate::objc::ownership::Action::Return(value) => cpu.x[0] = value,
                        crate::objc::ownership::Action::Dealloc {
                            object,
                            selector,
                            imp,
                        } => {
                            continuation = Some((cpu.clone(), object));
                            cpu.x[0] = object;
                            cpu.x[1] = selector;
                            cpu.x[30] = crate::objc::ownership::RETURN;
                            cpu.pc = imp;
                        }
                    }
                    continue;
                }
                if word == crate::objc::GET_CLASS
                    || word == crate::objc::LOOKUP
                    || word == crate::objc::SUPER_LOOKUP
                    || crate::objc::protocol::is_service(word)
                    || crate::objc::instances::is_service(word)
                {
                    let plan = self.image.objc.as_ref().ok_or("unregistered ObjC trap")?;
                    let result = plan.trap(memory, trap, word, cpu.x[0], cpu.x[1], cpu.x[17])?;
                    if word != crate::objc::LOOKUP && word != crate::objc::SUPER_LOOKUP {
                        cpu.x[0] = result;
                    } else {
                        cpu.x[16] = result;
                    }
                    continue;
                }
                *syscalls += 1;
                if let Some(exit) = syscall(cpu, memory, out, *started)? {
                    if continuation.is_some() {
                        return Err("guest exit during dealloc".into());
                    }
                    return Ok(Some(exit));
                }
            }
        }
    }
}

pub fn run(image: &Image, budget: u64, jit: bool) -> Result<Outcome> {
    Prepared::new(image, jit)?.run(budget)
}

#[cfg(test)]
mod ownership_execution_tests {
    use super::*;
    fn image() -> Image {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/bin/ownership/absolute/chained");
        let raw = std::fs::read(dir.join("client.macho")).unwrap();
        let libs = [
            "CoreFoundation.dylib",
            "Foundation.dylib",
            "libsable_objc.dylib",
            "libsable_system.dylib",
        ]
        .map(|n| std::fs::read(dir.join(n)).unwrap());
        Image::link_main(&raw, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap()
    }
    fn patch(image: &mut Image, a: u64, words: &[u32]) {
        let s = image
            .segments
            .iter_mut()
            .find(|s| {
                a >= s.address && a + (words.len() * 4) as u64 <= s.address + s.bytes.len() as u64
            })
            .unwrap();
        let at = (a - s.address) as usize;
        for (i, w) in words.iter().enumerate() {
            std::sync::Arc::make_mut(&mut s.bytes)[at + i * 4..at + i * 4 + 4]
                .copy_from_slice(&w.to_le_bytes());
        }
    }
    fn failed(p: &Prepared<'_>) -> (Cpu, Memory, Vec<u8>, String) {
        let mut process = Process {
            cpu: Cpu::new(p.image.entry, 30000),
            memory: Memory::new(p.image).unwrap(),
            out: [Vec::new(), Vec::new()],
            syscalls: 0,
            started: Instant::now(),
        };
        let args = process
            .arguments(p.image.startup.as_ref().unwrap().header)
            .unwrap();
        process.call(p.image.entry, &args);
        let err = p.drive(&mut process, Some(CALL_RETURN)).unwrap_err();
        (process.cpu, process.memory, process.out[0].clone(), err)
    }
    fn dealloc(image: &Image) -> u64 {
        let arena_class = 0x72000000;
        let mut memory = Memory::new(image).unwrap();
        // The first created object is the receiver for the original uppercase operation.
        // Resolve its class via the actual post-fixup class reference in the original Foundation metadata.
        let plan = image.objc.as_ref().unwrap();
        // Discover NSString using the existing checked runtime lookup, no host method implementation.
        let name = image
            .segments
            .iter()
            .find_map(|s| {
                s.bytes
                    .windows(9)
                    .position(|b| b == b"NSString\0")
                    .map(|n| s.address + n as u64)
            })
            .unwrap();
        let get = image
            .code_ranges()
            .flat_map(|(a, n)| (a..a + n).step_by(4))
            .find(|&a| image.instruction(a).unwrap() == crate::objc::GET_CLASS)
            .unwrap();
        let class = plan
            .trap(&mut memory, get, crate::objc::GET_CLASS, name, 0, 0)
            .unwrap();
        assert_eq!(memory.create_instance(class, 32, 1).unwrap(), arena_class);
        plan.dealloc_target(class).unwrap().1
    }
    #[test]
    fn checked_dealloc_return_disposal_reentrancy_and_precise_fault_state() {
        let image = image();
        let hook = dealloc(&image);
        let mut skip = image.clone();
        patch(&mut skip, hook, &[0xd65f03c0]);
        assert!(
            failed(&Prepared::new(&skip, false).unwrap())
                .3
                .contains("without exactly one final disposal")
        );
        // A checked dealloc callback cannot push a pool or invoke another ownership service.
        let push = image
            .code_ranges()
            .flat_map(|(a, n)| (a..a + n).step_by(4))
            .find(|&a| image.instruction(a).unwrap() == crate::objc::ownership::word(0))
            .unwrap();
        let mut reentrant = image.clone();
        let mut jump = Vec::new();
        for shift in 0..4 {
            jump.push(
                (if shift == 0 { 0xd2800010 } else { 0xf2800010 })
                    | shift << 21
                    | (((push >> (shift * 16)) & 65535) as u32) << 5,
            );
        }
        jump.push(0xd61f0200);
        patch(&mut reentrant, hook, &jump);
        assert!(
            failed(&Prepared::new(&reentrant, false).unwrap())
                .3
                .contains("lifecycle reentrancy")
        );
        // Keep LR intact but deliberately change SP before the callback RET.
        let mut unbalanced = image.clone();
        patch(&mut unbalanced, hook, &[0xd10043ff, 0xd65f03c0]);
        assert!(
            failed(&Prepared::new(&unbalanced, false).unwrap())
                .3
                .contains("unbalanced SP")
        );
        // Inject a guest store to read-only image bytes during the real dealloc continuation.
        let readonly = image
            .segments
            .iter()
            .find(|s| s.protection == 1)
            .unwrap()
            .address;
        let address = hook + 0x01000000;
        let mut words = Vec::new();
        for shift in 0..4 {
            words.push(
                (if shift == 0 { 0xd2800009 } else { 0xf2800009 })
                    | shift << 21
                    | (((readonly >> (shift * 16)) & 65535) as u32) << 5,
            );
        }
        words.push(0xf900013f);
        let code: Vec<_> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let mut fault = image.clone();
        fault.extra_text.push((address, code.len() as u64));
        fault.segments.push(crate::macho::Segment {
            address,
            bytes: code.into(),
            protection: 5,
        });
        patch(
            &mut fault,
            hook,
            &[0x14000000 | (((address - hook) / 4) as u32)],
        );
        let first = failed(&Prepared::new(&fault, false).unwrap());
        assert!(first.3.contains("permission 2"));
        assert_eq!(first.0.pc, address + 16);
        assert_eq!(first.2, b"SABLE FOUNDATION\n");
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        for dispatch in [false, true] {
            let other = failed(&Prepared::new_with_dispatch(&fault, dispatch).unwrap());
            assert!(other.3.starts_with("guest memory/SP fault at PC "));
            assert_eq!(other.0, first.0);
            assert_eq!(other.1, first.1);
            assert_eq!(other.2, first.2);
            for bad in [&skip, &reentrant, &unbalanced] {
                assert_eq!(
                    failed(&Prepared::new_with_dispatch(bad, dispatch).unwrap()),
                    failed(&Prepared::new(bad, false).unwrap())
                );
            }
        }
        assert_eq!(
            Prepared::new(&image, false)
                .unwrap()
                .run(30000)
                .unwrap()
                .exit,
            0
        );
    }
    #[test]
    fn dealloc_continuation_detects_d8_clobber_and_restores_v_registers() {
        let image = image();
        let hook = dealloc(&image);
        // FMOV d8, #1.0 then fall into the original dealloc: violates AAPCS64.
        let mut clobber = image.clone();
        patch(&mut clobber, hook, &[0x1e6e_1008, 0xd65f_03c0]);
        let (cpu, _, _, err) = failed(&Prepared::new(&clobber, false).unwrap());
        assert!(err.contains("clobbered callee-saved d8"), "{err}");
        assert_eq!(cpu.v[8] as u64, 1f64.to_bits());
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        for dispatch in [false, true] {
            let other = failed(&Prepared::new_with_dispatch(&clobber, dispatch).unwrap());
            assert_eq!(other.3, err);
            assert_eq!(other.0, cpu);
        }
        // Caller-saved d0 and d16 may change inside dealloc; the caller still sees
        // its own V state because the continuation restores v0-v31.
        let mut scratch = image.clone();
        patch(&mut scratch, hook, &[0x1e6e_1000, 0x1e6e_1010, 0xd65f_03c0]);
        // The patched dealloc skips disposal, so the run fails, but only for that reason.
        let err = failed(&Prepared::new(&scratch, false).unwrap()).3;
        assert!(err.contains("without exactly one final disposal"), "{err}");
    }
}
