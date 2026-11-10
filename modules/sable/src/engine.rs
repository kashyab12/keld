use crate::{
    Result,
    isa::{Op, decode},
    macho::Image,
};
use std::time::Instant;

// Layout is consumed by the x86 backend. Keep offsets checked in its tests.
#[repr(C)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cpu {
    pub x: [u64; 31],
    pub sp: u64,
    pub pc: u64,
    pub remaining: u64,
}

impl Cpu {
    pub fn new(entry: u64, budget: u64) -> Self {
        Self {
            x: [0; 31],
            sp: 0,
            pc: entry,
            remaining: budget,
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

pub fn step(cpu: &mut Cpu, image: &Image) -> Result<bool> {
    if cpu.remaining == 0 {
        return Err("instruction budget exhausted".into());
    }
    let op = decode(image.instruction(cpu.pc)?, cpu.pc)?;
    cpu.remaining -= 1;
    let mut next = cpu.pc + 4;
    match op {
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
        } => {
            let lhs = cpu.reg(src, true);
            cpu.set(
                dst,
                if subtract {
                    lhs.wrapping_sub(value)
                } else {
                    lhs.wrapping_add(value)
                },
                wide,
                true,
            );
        }
        Op::AddReg {
            dst,
            lhs,
            rhs,
            subtract,
            wide,
        } => {
            let a = cpu.reg(lhs, false);
            let b = cpu.reg(rhs, false);
            cpu.set(
                dst,
                if subtract {
                    a.wrapping_sub(b)
                } else {
                    a.wrapping_add(b)
                },
                wide,
                false,
            );
        }
        Op::OrReg {
            dst,
            lhs,
            rhs,
            wide,
        } => cpu.set(dst, cpu.reg(lhs, false) | cpu.reg(rhs, false), wide, false),
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
        Op::Svc => {
            cpu.pc = next;
            return Ok(true);
        }
        Op::Nop => {}
    }
    cpu.pc = next;
    Ok(false)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub cpu: Cpu,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit: u8,
    pub syscalls: u64,
}

/// Deliberately tiny Darwin contract, not arbitrary host syscall forwarding.
/// Unknown calls fail explicitly: pretending ENOSYS success hides missing ABI work.
fn syscall(
    cpu: &mut Cpu,
    image: &Image,
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
            out[channel].extend_from_slice(image.read(cpu.x[1], len, 1)?);
            cpu.x[0] = len as u64;
            Ok(None)
        }
        // A deterministic process identity for our single-process test environment.
        20 => {
            cpu.x[0] = 1;
            Ok(None)
        }
        // mach_absolute_time trap (-3), timebase numerator/denominator = 1 here.
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

pub fn run(image: &Image, budget: u64, jit: bool) -> Result<Outcome> {
    let mut cpu = Cpu::new(image.entry, budget);
    let mut out = [Vec::new(), Vec::new()];
    let mut syscalls = 0;
    let started = Instant::now();
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
    loop {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let trapped = if let Some(code) = &compiled {
            code.execute(&mut cpu)?;
            true
        } else {
            step(&mut cpu, image)?
        };
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        let trapped = step(&mut cpu, image)?;
        if trapped {
            syscalls += 1;
            if let Some(exit) = syscall(&mut cpu, image, &mut out, started)? {
                let [stdout, stderr] = out;
                return Ok(Outcome {
                    cpu,
                    stdout,
                    stderr,
                    exit,
                    syscalls,
                });
            }
        }
    }
}
