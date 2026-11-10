//! ARM64 -> x86-64 static block translation with direct branch linking.
//! No host helper calls from generated code. Every instruction consumes a budget.
use crate::{
    Result,
    engine::{Cpu, condition_holds, step_memory},
    isa::{Op, decode},
    macho::Image,
    memory::{Memory, STACK_BASE, STACK_SIZE},
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
const NZCV: u32 = 272;

#[derive(Default)]
struct Assembler {
    bytes: Vec<u8>,
    labels: HashMap<u64, usize>,
    branches: Vec<(usize, u64)>,
    budgets: Vec<usize>,
    layout: Vec<(u64, usize, u32)>,
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
    fn shift_rhs(&mut self, shift: u8, amount: u8, wide: bool) {
        if amount == 0 {
            return;
        }
        if wide {
            self.emit(&[0x48]);
        }
        self.emit(&[
            0xc1,
            match shift {
                0 => 0xe2,
                1 => 0xea,
                2 => 0xfa,
                _ => 0xca,
            },
            amount,
        ]);
    }
    fn flags(&mut self, subtract: bool) {
        // Capture all flags before any shifts/ORs alter host EFLAGS.
        self.emit(&[
            0x0f,
            0x90,
            0xc2,
            0x0f,
            if subtract { 0x93 } else { 0x92 },
            0xc1,
            0x41,
            0x0f,
            0x94,
            0xc0,
            0x41,
            0x0f,
            0x98,
            0xc1,
        ]);
        self.emit(&[
            0x0f, 0xb6, 0xd2, 0x0f, 0xb6, 0xc9, 0xc1, 0xe1, 1, 0x09, 0xca,
        ]);
        self.emit(&[
            0x45, 0x0f, 0xb6, 0xc0, 0x41, 0xc1, 0xe0, 2, 0x44, 0x09, 0xc2,
        ]);
        self.emit(&[
            0x45, 0x0f, 0xb6, 0xc9, 0x41, 0xc1, 0xe1, 3, 0x44, 0x09, 0xca,
        ]);
        self.emit(&[0x48, 0xc1, 0xe2, 28, 0x48, 0x89, 0x97]);
        self.emit(&NZCV.to_le_bytes());
    }
    fn local_jump(&mut self, condition: Option<u8>) -> usize {
        if let Some(condition) = condition {
            self.emit(&[0x0f, condition]);
        } else {
            self.emit(&[0xe9]);
        }
        let at = self.bytes.len();
        self.emit(&[0; 4]);
        at
    }
    fn memory(&mut self, op: Op) -> Result<()> {
        let (reg, base, bytes, load, offset, post, writeback, index) = match op {
            Op::Memory {
                reg,
                base,
                bytes,
                load,
                offset,
                post,
                writeback,
            } => (reg, base, bytes, load, offset, post, writeback, None),
            Op::MemoryReg {
                reg,
                base,
                bytes,
                load,
                index,
                option,
                scaled,
            } => (
                reg,
                base,
                bytes,
                load,
                0,
                false,
                false,
                Some((index, option, scaled)),
            ),
            _ => unreachable!(),
        };
        self.load(base, true, false);
        let alignment = if base == 31 {
            self.emit(&[0xa8, 15]);
            Some(self.local_jump(Some(0x85)))
        } else {
            None
        };
        if let Some((index, option, scaled)) = index {
            self.load(index, false, true);
            match option {
                2 => self.emit(&[0x89, 0xd2]),
                6 => self.emit(&[0x48, 0x63, 0xd2]),
                _ => {}
            }
            if scaled {
                self.shift_rhs(0, bytes.trailing_zeros() as u8, true);
            }
            self.emit(&[0x48, 0x01, 0xd0]);
        } else if !post && offset != 0 {
            self.imm64(offset as u64, true);
            self.emit(&[0x48, 0x01, 0xd0]);
        }
        let mut completed = Vec::new();
        for (index, (address, length, permission)) in self.layout.clone().into_iter().enumerate() {
            if permission & if load { 1 } else { 2 } == 0 || length < bytes as usize {
                continue;
            }
            self.imm64(address, true);
            self.emit(&[0x48, 0x39, 0xd0]);
            let low = self.local_jump(Some(0x82));
            self.imm64(address + length as u64 - u64::from(bytes), true);
            self.emit(&[0x48, 0x39, 0xd0]);
            let high = self.local_jump(Some(0x87));
            // RAX guest address -> checked host pointer, table passed exclusively in RSI.
            self.emit(&[0x48, 0x8b, 0x96]);
            self.emit(&((index * 8) as u32).to_le_bytes());
            self.emit(&[0x49, 0xbb]);
            self.emit(&address.to_le_bytes());
            self.emit(&[0x4c, 0x29, 0xd8, 0x48, 0x01, 0xd0]);
            if load {
                match bytes {
                    1 => self.emit(&[0x0f, 0xb6, 0x10]),
                    2 => self.emit(&[0x0f, 0xb7, 0x10]),
                    4 => self.emit(&[0x8b, 0x10]),
                    8 => self.emit(&[0x48, 0x8b, 0x10]),
                    _ => unreachable!(),
                }
                self.emit(&[0x48, 0x89, 0xd0]);
                self.store(reg, false, true);
            } else {
                self.load(reg, false, true);
                match bytes {
                    1 => self.emit(&[0x88, 0x10]),
                    2 => self.emit(&[0x66, 0x89, 0x10]),
                    4 => self.emit(&[0x89, 0x10]),
                    8 => self.emit(&[0x48, 0x89, 0x10]),
                    _ => unreachable!(),
                }
            }
            if writeback {
                self.load(base, true, false);
                self.imm64(offset as u64, true);
                self.emit(&[0x48, 0x01, 0xd0]);
                self.store(base, true, true);
            }
            self.emit(&[0x48, 0x83, 0xaf]);
            self.emit(&REMAINING.to_le_bytes());
            self.emit(&[1]);
            completed.push(self.local_jump(None));
            let next = self.bytes.len();
            self.patch(low, next)?;
            self.patch(high, next)?;
        }
        let fault = self.bytes.len();
        self.exit(5);
        if let Some(at) = alignment {
            self.patch(at, fault)?;
        }
        let end = self.bytes.len();
        for at in completed {
            self.patch(at, end)?;
        }
        Ok(())
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
                flags,
            } => {
                self.load(src, true, false);
                self.imm64(value, true);
                if wide {
                    self.emit(&[0x48]);
                }
                self.emit(&[if subtract { 0x29 } else { 0x01 }, 0xd0]);
                if flags {
                    self.flags(subtract);
                }
                self.store(dst, !flags, wide);
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
                self.load(lhs, false, false);
                self.load(rhs, false, true);
                self.shift_rhs(shift, amount, wide);
                if wide {
                    self.emit(&[0x48]);
                }
                self.emit(&[if subtract { 0x29 } else { 0x01 }, 0xd0]);
                if flags {
                    self.flags(subtract);
                }
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
                self.load(lhs, false, false);
                self.load(rhs, false, true);
                self.shift_rhs(shift, amount, wide);
                if invert {
                    self.emit(&[0x48, 0xf7, 0xd2]);
                }
                if wide {
                    self.emit(&[0x48]);
                }
                self.emit(&[
                    match kind {
                        0 | 3 => 0x21,
                        1 => 0x09,
                        _ => 0x31,
                    },
                    0xd0,
                ]);
                if kind == 3 {
                    self.flags(false);
                }
                self.store(dst, false, wide);
            }
            Op::CondBranch { target, condition } => {
                let mask = (0..16).fold(0u32, |m, n| {
                    m | ((condition_holds(n << 28, condition) as u32) << n)
                });
                self.emit(&[0x8b, 0x8f]);
                self.emit(&NZCV.to_le_bytes());
                self.emit(&[0xc1, 0xe9, 28, 0xb8]);
                self.emit(&mask.to_le_bytes());
                self.emit(&[0xd3, 0xe8, 0xa8, 1]);
                self.branch(target, Some(true));
            }
            Op::Memory { .. } | Op::MemoryReg { .. } | Op::Pair { .. } => {
                unreachable!("memory operations use checked Rust transition")
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
    layout: Vec<(u64, usize, u32)>,
}

impl Compiled {
    pub fn new(image: &Image) -> Result<Self> {
        let mut layout: Vec<_> = image
            .segments
            .iter()
            .map(|s| (s.address, s.bytes.len(), s.protection))
            .collect();
        layout.push((STACK_BASE, STACK_SIZE, 3));
        let mut a = Assembler {
            layout: layout.clone(),
            ..Assembler::default()
        };
        for pc in (image.text.0..image.text.0 + image.text.1).step_by(4) {
            a.labels.insert(pc, a.bytes.len());
            // Invalid instructions fault only if reached, matching the interpreter.
            match decode(image.instruction(pc)?, pc) {
                Ok(op @ (Op::Memory { .. } | Op::MemoryReg { .. })) => {
                    a.guard(pc);
                    a.memory(op)?;
                }
                Ok(Op::Pair { .. }) => {
                    a.guard(pc);
                    a.exit(4);
                }
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
            layout,
        })
    }

    pub fn execute_memory(&self, cpu: &mut Cpu, image: &Image, memory: &mut Memory) -> Result<()> {
        if memory.layout() != self.layout {
            return Err("native memory layout mismatch".into());
        }
        let pointers: Vec<*mut u8> = memory
            .regions
            .iter_mut()
            .map(|s| s.bytes.as_mut_ptr())
            .collect();
        loop {
            if cpu.remaining == 0 {
                return Err("instruction budget exhausted".into());
            }
            let offset = *self
                .entries
                .get(&cpu.pc)
                .ok_or_else(|| format!("PC outside __text: {:#x}", cpu.pc))?;
            // SAFETY: each entry starts generated SysV code taking *mut Cpu in RDI;
            // it accesses Cpu and bounds-checked guest buffers via the exclusive pointer table,
            // preserves callee-saved registers, and returns
            // u32. Code stays RX; buffers and pointer table stay allocated through the call.
            // No Rust references access the buffers during native execution; no host calls.
            let reason = unsafe {
                let entry: unsafe extern "C" fn(*mut Cpu, *const *mut u8) -> u32 =
                    std::mem::transmute(self.memory.as_ptr().cast::<u8>().add(offset));
                entry(cpu, pointers.as_ptr())
            };
            match reason {
                0 => return Ok(()),
                2 => return Err("instruction budget exhausted".into()),
                3 => continue,
                5 => return Err(format!("guest memory/SP fault at PC {:#x}", cpu.pc)),
                4 => {
                    step_memory(cpu, image, memory)?;
                    continue;
                }
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
        assert_eq!(std::mem::offset_of!(Cpu, nzcv), NZCV as usize);
        assert_eq!(std::mem::size_of::<Cpu>(), 280);
    }
}
