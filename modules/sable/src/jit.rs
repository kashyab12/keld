//! ARM64 -> x86-64 static block translation with direct branch linking.
//! No host helper calls from generated code. Every instruction consumes a budget.
use crate::{
    Result,
    engine::Cpu,
    isa::{Op, decode},
    macho::Image,
};
use std::{collections::HashMap, ffi::c_void, ptr::NonNull};

unsafe extern "C" {
    fn mmap(addr: *mut c_void, len: usize, prot: i32, flags: i32, fd: i32, off: i64)
    -> *mut c_void;
    fn mprotect(addr: *mut c_void, len: usize, prot: i32) -> i32;
    fn munmap(addr: *mut c_void, len: usize) -> i32;
}

const PC: u32 = 256;
const REMAINING: u32 = 264;

#[derive(Default)]
struct Assembler {
    bytes: Vec<u8>,
    labels: HashMap<u64, usize>,
    branches: Vec<(usize, u64)>,
    budgets: Vec<usize>,
}

impl Assembler {
    fn emit(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }
    fn imm64(&mut self, value: u64, rhs: bool) {
        self.emit(if rhs { &[0x48, 0xba] } else { &[0x48, 0xb8] });
        self.emit(&value.to_le_bytes());
    }
    fn load(&mut self, reg: u8, sp: bool, rhs: bool) {
        if reg == 31 && !sp {
            self.emit(if rhs { &[0x31, 0xd2] } else { &[0x31, 0xc0] });
        } else {
            self.emit(if rhs {
                &[0x48, 0x8b, 0x97]
            } else {
                &[0x48, 0x8b, 0x87]
            });
            self.emit(&(u32::from(reg) * 8).to_le_bytes());
        }
    }
    fn store_at(&mut self, offset: u32) {
        self.emit(&[0x48, 0x89, 0x87]);
        self.emit(&offset.to_le_bytes());
    }
    fn store(&mut self, reg: u8, sp: bool, wide: bool) {
        if reg == 31 && !sp {
            return;
        }
        if !wide {
            self.emit(&[0x89, 0xc0]);
        }
        self.store_at(u32::from(reg) * 8);
    }
    fn pc(&mut self, pc: u64) {
        self.imm64(pc, false);
        self.store_at(PC);
    }
    fn exit(&mut self, reason: u32) {
        self.emit(&[0xb8]);
        self.emit(&reason.to_le_bytes());
        self.emit(&[0xc3]);
    }
    fn branch(&mut self, target: u64, condition: Option<bool>) {
        if let Some(nonzero) = condition {
            self.emit(&[0x0f, if nonzero { 0x85 } else { 0x84 }]);
        } else {
            self.emit(&[0xe9]);
        }
        self.branches.push((self.bytes.len(), target));
        self.emit(&[0; 4]);
    }
    fn guard(&mut self, pc: u64) {
        self.pc(pc);
        self.emit(&[0x48, 0x83, 0xbf]);
        self.emit(&REMAINING.to_le_bytes());
        self.emit(&[0]);
        self.emit(&[0x0f, 0x84]);
        self.budgets.push(self.bytes.len());
        self.emit(&[0; 4]);
    }
    fn budget(&mut self, pc: u64) {
        self.guard(pc);
        self.emit(&[0x48, 0x83, 0xaf]);
        self.emit(&REMAINING.to_le_bytes());
        self.emit(&[1]);
    }
    fn instruction(&mut self, op: Op, pc: u64) {
        match op {
            Op::MovWide {
                dst,
                value,
                keep,
                shift,
                wide,
            } => {
                if keep {
                    self.load(dst, false, false);
                    self.imm64(!(0xffffu64 << shift), true);
                    self.emit(&[0x48, 0x21, 0xd0]);
                    self.imm64(value, true);
                    self.emit(&[0x48, 0x09, 0xd0]);
                } else {
                    self.imm64(value, false);
                }
                self.store(dst, false, wide);
            }
            Op::AddImm {
                dst,
                src,
                value,
                subtract,
                wide,
            } => {
                self.load(src, true, false);
                self.imm64(value, true);
                self.emit(&[0x48, if subtract { 0x29 } else { 0x01 }, 0xd0]);
                self.store(dst, true, wide);
            }
            Op::AddReg {
                dst,
                lhs,
                rhs,
                subtract,
                wide,
            } => {
                self.load(lhs, false, false);
                self.load(rhs, false, true);
                self.emit(&[0x48, if subtract { 0x29 } else { 0x01 }, 0xd0]);
                self.store(dst, false, wide);
            }
            Op::OrReg {
                dst,
                lhs,
                rhs,
                wide,
            } => {
                self.load(lhs, false, false);
                self.load(rhs, false, true);
                self.emit(&[0x48, 0x09, 0xd0]);
                self.store(dst, false, wide);
            }
            Op::Address { dst, value } => {
                self.imm64(value, false);
                self.store(dst, false, true);
            }
            Op::Branch { target, link } => {
                if link {
                    self.imm64(pc + 4, false);
                    self.store(30, false, true);
                }
                self.branch(target, None);
            }
            Op::CompareBranch {
                reg,
                target,
                nonzero,
                wide,
            } => {
                self.load(reg, false, false);
                if wide {
                    self.emit(&[0x48]);
                }
                self.emit(&[0x85, 0xc0]);
                self.branch(target, Some(nonzero));
            }
            Op::Return { reg } => {
                self.load(reg, false, false);
                self.store_at(PC);
                self.exit(3);
            }
            Op::Svc => {
                self.pc(pc + 4);
                self.exit(0);
            }
            Op::Nop => {}
        }
    }
    fn patch(&mut self, at: usize, target: usize) -> Result<()> {
        let relative = i32::try_from(target as i64 - (at + 4) as i64)
            .map_err(|_| "native branch exceeds rel32")?;
        self.bytes[at..at + 4].copy_from_slice(&relative.to_le_bytes());
        Ok(())
    }
}

pub struct Compiled {
    memory: NonNull<c_void>,
    length: usize,
    entries: HashMap<u64, usize>,
}

impl Compiled {
    pub fn new(image: &Image) -> Result<Self> {
        let mut a = Assembler::default();
        for pc in (image.text.0..image.text.0 + image.text.1).step_by(4) {
            a.labels.insert(pc, a.bytes.len());
            // Invalid instructions fault only if reached, matching the interpreter.
            match decode(image.instruction(pc)?, pc) {
                Ok(op) => {
                    a.budget(pc);
                    a.instruction(op, pc);
                }
                Err(_) => {
                    a.guard(pc);
                    a.exit(1);
                }
            }
        }
        a.guard(image.text.0 + image.text.1);
        a.exit(1);
        let exhausted = a.bytes.len();
        a.exit(2);
        for (at, target) in a.branches.clone() {
            let destination = if let Some(offset) = a.labels.get(&target) {
                *offset
            } else {
                // Record the actual faulting guest PC, even for a branch out of text.
                let destination = a.bytes.len();
                a.guard(target);
                a.exit(1);
                destination
            };
            a.patch(at, destination)?;
        }
        for at in a.budgets.clone() {
            a.patch(at, exhausted)?;
        }
        let length = a.bytes.len();
        // SAFETY: anonymous, private RW mapping owned by this object. No guest pointer
        // is used. Linux rounds mapping length to pages; MAP_FAILED is checked.
        let raw = unsafe { mmap(std::ptr::null_mut(), length, 3, 0x22, -1, 0) };
        if raw as isize == -1 {
            return Err(format!("mmap: {}", std::io::Error::last_os_error()));
        }
        let memory = NonNull::new(raw).ok_or("mmap returned null")?;
        // SAFETY: mapping is writable for length bytes; generated bytes fit exactly.
        unsafe {
            std::ptr::copy_nonoverlapping(a.bytes.as_ptr(), raw.cast(), length);
        }
        // SAFETY: same owned mapping, transition to RX. Never RWX.
        if unsafe { mprotect(raw, length, 5) } != 0 {
            let error = std::io::Error::last_os_error();
            unsafe {
                munmap(raw, length);
            }
            return Err(format!("mprotect: {error}"));
        }
        Ok(Self {
            memory,
            length,
            entries: a.labels,
        })
    }

    pub fn execute(&self, cpu: &mut Cpu) -> Result<()> {
        loop {
            if cpu.remaining == 0 {
                return Err("instruction budget exhausted".into());
            }
            let offset = *self
                .entries
                .get(&cpu.pc)
                .ok_or_else(|| format!("PC outside __text: {:#x}", cpu.pc))?;
            // SAFETY: each entry starts generated SysV code taking *mut Cpu in RDI;
            // it touches only Cpu fields, preserves callee-saved registers, and returns
            // u32. Mapping stays RX and alive throughout the call. There are no guest
            // memory accesses or host calls in generated code.
            let reason = unsafe {
                let entry: unsafe extern "C" fn(*mut Cpu) -> u32 =
                    std::mem::transmute(self.memory.as_ptr().cast::<u8>().add(offset));
                entry(cpu)
            };
            match reason {
                0 => return Ok(()),
                2 => return Err("instruction budget exhausted".into()),
                3 => continue,
                _ => return Err(format!("unsupported instruction or PC at {:#x}", cpu.pc)),
            }
        }
    }
}

impl Drop for Compiled {
    fn drop(&mut self) {
        // SAFETY: unique owner; no generated call can outlive &self.
        unsafe {
            munmap(self.memory.as_ptr(), self.length);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_layout_matches_emitter() {
        assert_eq!(std::mem::offset_of!(Cpu, sp), 248);
        assert_eq!(std::mem::offset_of!(Cpu, pc), PC as usize);
        assert_eq!(std::mem::offset_of!(Cpu, remaining), REMAINING as usize);
        assert_eq!(std::mem::size_of::<Cpu>(), 272);
    }
}
