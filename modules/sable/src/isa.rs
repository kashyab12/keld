use crate::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    MovWide {
        dst: u8,
        value: u64,
        keep: bool,
        shift: u32,
        wide: bool,
    },
    AddImm {
        dst: u8,
        src: u8,
        value: u64,
        subtract: bool,
        wide: bool,
    },
    AddReg {
        dst: u8,
        lhs: u8,
        rhs: u8,
        subtract: bool,
        wide: bool,
    },
    OrReg {
        dst: u8,
        lhs: u8,
        rhs: u8,
        wide: bool,
    },
    Address {
        dst: u8,
        value: u64,
    },
    Branch {
        target: u64,
        link: bool,
    },
    CompareBranch {
        reg: u8,
        target: u64,
        nonzero: bool,
        wide: bool,
    },
    Return {
        reg: u8,
    },
    Svc,
    Nop,
}

fn signed(value: u32, bits: u32) -> i64 {
    ((value as i64) << (64 - bits)) >> (64 - bits)
}

pub fn decode(word: u32, pc: u64) -> Result<Op> {
    let wide = word >> 31 != 0;
    let dst = (word & 31) as u8;
    let src = ((word >> 5) & 31) as u8;
    let rhs = ((word >> 16) & 31) as u8;
    if word == 0xd503201f {
        return Ok(Op::Nop);
    }
    if word & 0xffe0001f == 0xd4000001 {
        if (word >> 5) & 0xffff != 0x80 {
            return Err("only Darwin svc #0x80 supported".into());
        }
        return Ok(Op::Svc);
    }
    if word & 0xfffffc1f == 0xd65f0000 {
        return Ok(Op::Return { reg: src });
    }
    if word & 0x1f800000 == 0x12800000 {
        let opc = (word >> 29) & 3;
        let shift = ((word >> 21) & 3) * 16;
        if (!wide && shift >= 32) || opc == 1 {
            return Err("reserved move-wide encoding".into());
        }
        let mut value = (((word >> 5) & 0xffff) as u64) << shift;
        if opc == 0 {
            value = !value;
        }
        return Ok(Op::MovWide {
            dst,
            value,
            keep: opc == 3,
            shift,
            wide,
        });
    }
    if word & 0x1f800000 == 0x11000000 && word & (1 << 29) == 0 {
        return Ok(Op::AddImm {
            dst,
            src,
            value: (((word >> 10) & 4095) as u64) << (if word & (1 << 22) != 0 { 12 } else { 0 }),
            subtract: word & (1 << 30) != 0,
            wide,
        });
    }
    if word & 0x1fe0fc00 == 0x0b000000 && word & (1 << 29) == 0 {
        return Ok(Op::AddReg {
            dst,
            lhs: src,
            rhs,
            subtract: word & (1 << 30) != 0,
            wide,
        });
    }
    if word & 0x7fe0fc00 == 0x2a000000 {
        return Ok(Op::OrReg {
            dst,
            lhs: src,
            rhs,
            wide,
        });
    }
    if word & 0x1f000000 == 0x10000000 {
        let immediate = signed(((word >> 5) & 0x7ffff) << 2 | ((word >> 29) & 3), 21);
        let value = if wide {
            (pc & !4095).wrapping_add_signed(immediate << 12)
        } else {
            pc.wrapping_add_signed(immediate)
        };
        return Ok(Op::Address { dst, value });
    }
    if word & 0x7c000000 == 0x14000000 {
        return Ok(Op::Branch {
            target: pc.wrapping_add_signed(signed(word & 0x03ffffff, 26) << 2),
            link: wide,
        });
    }
    if word & 0x7e000000 == 0x34000000 {
        return Ok(Op::CompareBranch {
            reg: dst,
            target: pc.wrapping_add_signed(signed((word >> 5) & 0x7ffff, 19) << 2),
            nonzero: word & (1 << 24) != 0,
            wide,
        });
    }
    Err(format!(
        "unsupported ARM64 instruction {word:#010x} at {pc:#x}"
    ))
}
