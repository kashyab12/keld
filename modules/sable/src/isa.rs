use crate::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Indirect {
        reg: u8,
        link: bool,
    },
    Literal {
        reg: u8,
        address: u64,
        bytes: u8,
    },
    MemoryReg {
        reg: u8,
        base: u8,
        index: u8,
        option: u8,
        scaled: bool,
        bytes: u8,
        load: bool,
    },
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
        flags: bool,
    },
    AddReg {
        dst: u8,
        lhs: u8,
        rhs: u8,
        subtract: bool,
        wide: bool,
        flags: bool,
        shift: u8,
        amount: u8,
    },
    OrReg {
        dst: u8,
        lhs: u8,
        rhs: u8,
        wide: bool,
    },
    Logical {
        dst: u8,
        lhs: u8,
        rhs: u8,
        kind: u8,
        invert: bool,
        shift: u8,
        amount: u8,
        wide: bool,
    },
    LogicalImm {
        dst: u8,
        src: u8,
        value: u64,
        kind: u8,
        wide: bool,
    },
    UnsignedBitfield {
        dst: u8,
        src: u8,
        right: u8,
        left: u8,
        mask: u64,
        wide: bool,
    },
    CondSelect {
        dst: u8,
        lhs: u8,
        rhs: u8,
        condition: u8,
        invert: bool,
        increment: bool,
        wide: bool,
    },
    CondCompare {
        lhs: u8,
        rhs: u8,
        immediate: bool,
        subtract: bool,
        wide: bool,
        condition: u8,
        nzcv: u8,
    },
    CondBranch {
        target: u64,
        condition: u8,
    },
    Memory {
        reg: u8,
        base: u8,
        offset: i64,
        bytes: u8,
        load: bool,
        writeback: bool,
        post: bool,
    },
    Pair {
        first: u8,
        second: u8,
        base: u8,
        offset: i64,
        bytes: u8,
        load: bool,
        writeback: bool,
        post: bool,
    },
    Address {
        dst: u8,
        value: u64,
    },
    Branch {
        target: u64,
        link: bool,
    },
    TestBranch {
        reg: u8,
        bit: u8,
        target: u64,
        nonzero: bool,
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
    if word & 0x7f80_0000 == 0x5300_0000 {
        let n = word & (1 << 22) != 0;
        let r = ((word >> 16) & 63) as u8;
        let s = ((word >> 10) & 63) as u8;
        if n != wide || (!wide && (r >= 32 || s >= 32)) {
            return Err("reserved unsigned bitfield encoding".into());
        }
        let (right, left, bits) = if s >= r {
            (r, 0, s - r + 1)
        } else {
            (0, (if wide { 64 } else { 32 }) - r, s + 1)
        };
        return Ok(Op::UnsignedBitfield {
            dst,
            src,
            right,
            left,
            mask: u64::MAX >> (64 - bits),
            wide,
        });
    }
    if word & 0x7e000000 == 0x36000000 {
        return Ok(Op::TestBranch {
            reg: dst,
            bit: (((word >> 31) << 5) | ((word >> 19) & 31)) as u8,
            target: pc.wrapping_add_signed(signed((word >> 5) & 0x3fff, 14) << 2),
            nonzero: word & (1 << 24) != 0,
        });
    }
    if word & 0xfffffc1f == 0xd61f0000 || word & 0xfffffc1f == 0xd63f0000 {
        return Ok(Op::Indirect {
            reg: src,
            link: word & (1 << 21) != 0,
        });
    }
    if word & 0x3fe0_0800 == 0x1a80_0000 {
        return Ok(Op::CondSelect {
            dst,
            lhs: src,
            rhs,
            condition: ((word >> 12) & 15) as u8,
            invert: word & (1 << 30) != 0,
            increment: word & (1 << 10) != 0,
            wide,
        });
    }
    if word & 0x3b000000 == 0x18000000 && word & (1 << 26) == 0 {
        let opc = word >> 30;
        if opc > 1 {
            return Err("signed/prefetch literal load unsupported".into());
        }
        return Ok(Op::Literal {
            reg: dst,
            address: pc.wrapping_add_signed(signed((word >> 5) & 0x7ffff, 19) << 2),
            bytes: if opc == 0 { 4 } else { 8 },
        });
    }
    if word == 0xd503201f {
        return Ok(Op::Nop);
    }
    if word & 0x1f80_0000 == 0x1200_0000 {
        let n = (word >> 22) & 1;
        let imms = (word >> 10) & 63;
        let immr = (word >> 16) & 63;
        let selector = (n << 6) | (!imms & 63);
        if selector < 2 || (!wide && n != 0) {
            return Err("reserved logical immediate".into());
        }
        let width = 1u32 << (31 - selector.leading_zeros());
        let ones = (imms & (width - 1)) + 1;
        let rotate = immr & (width - 1);
        if ones == width || width > if wide { 64 } else { 32 } {
            return Err("reserved logical immediate mask".into());
        }
        let bits = (1u64 << ones) - 1;
        let mask = if width == 64 {
            u64::MAX
        } else {
            (1u64 << width) - 1
        };
        let element = if rotate == 0 {
            bits
        } else {
            ((bits >> rotate) | (bits << (width - rotate))) & mask
        };
        let mut value = 0;
        for shift in (0..if wide { 64 } else { 32 }).step_by(width as usize) {
            value |= element << shift;
        }
        return Ok(Op::LogicalImm {
            dst,
            src,
            value,
            kind: ((word >> 29) & 3) as u8,
            wide,
        });
    }
    if word & 0x3fe0_0410 == 0x3a40_0000 {
        return Ok(Op::CondCompare {
            lhs: src,
            rhs: ((word >> 16) & 31) as u8,
            immediate: word & (1 << 11) != 0,
            subtract: word & (1 << 30) != 0,
            wide,
            condition: ((word >> 12) & 15) as u8,
            nzcv: (word & 15) as u8,
        });
    }
    if word & 0xffe0001f == 0xd4000001 {
        if (word >> 5) & 0xffff != 0x80
            && word != crate::tls::SVC
            && word != crate::objc::GET_CLASS
            && word != crate::objc::LOOKUP
            && !crate::objc::protocol::is_service(word)
        {
            return Err("only Darwin svc #0x80 or registered Sable runtime traps supported".into());
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
    if word & 0x1f800000 == 0x11000000 {
        return Ok(Op::AddImm {
            dst,
            src,
            value: (((word >> 10) & 4095) as u64) << (if word & (1 << 22) != 0 { 12 } else { 0 }),
            subtract: word & (1 << 30) != 0,
            wide,
            flags: word & (1 << 29) != 0,
        });
    }
    if word & 0x1f200000 == 0x0b000000 {
        let shift = ((word >> 22) & 3) as u8;
        let amount = ((word >> 10) & 63) as u8;
        if shift == 3 || (!wide && amount >= 32) {
            return Err("reserved arithmetic shift".into());
        }
        return Ok(Op::AddReg {
            dst,
            lhs: src,
            rhs,
            subtract: word & (1 << 30) != 0,
            wide,
            flags: word & (1 << 29) != 0,
            shift,
            amount,
        });
    }
    if word & 0x1f000000 == 0x0a000000 {
        let shift = ((word >> 22) & 3) as u8;
        let amount = ((word >> 10) & 63) as u8;
        if !wide && amount >= 32 {
            return Err("reserved logical shift".into());
        }
        let kind = ((word >> 29) & 3) as u8;
        let invert = word & (1 << 21) != 0;
        if kind == 1 && !invert && shift == 0 && amount == 0 {
            return Ok(Op::OrReg {
                dst,
                lhs: src,
                rhs,
                wide,
            });
        }
        return Ok(Op::Logical {
            dst,
            lhs: src,
            rhs,
            kind,
            invert,
            shift,
            amount,
            wide,
        });
    }
    if word & 0xff000010 == 0x54000000 {
        return Ok(Op::CondBranch {
            target: pc.wrapping_add_signed(signed((word >> 5) & 0x7ffff, 19) << 2),
            condition: (word & 15) as u8,
        });
    }
    if word & 0x3b200c00 == 0x38200800 && word & (1 << 26) == 0 {
        let opc = (word >> 22) & 3;
        let option = ((word >> 13) & 7) as u8;
        if opc > 1 || option & 2 == 0 {
            return Err("unsupported register-offset load/store".into());
        }
        return Ok(Op::MemoryReg {
            reg: dst,
            base: src,
            index: rhs,
            option,
            scaled: word & (1 << 12) != 0,
            bytes: 1u8 << (word >> 30),
            load: opc == 1,
        });
    }
    // Integer unsigned-immediate and unscaled/pre/post-index loads/stores.
    if (word & 0x3b000000 == 0x39000000 || word & 0x3b200000 == 0x38000000) && word & (1 << 26) == 0
    {
        let bytes = 1u8 << (word >> 30);
        let opc = (word >> 22) & 3;
        if opc > 1 {
            return Err("signed/vector load/store unsupported".into());
        }
        let unsigned = word & (1 << 24) != 0;
        let mode = (word >> 10) & 3;
        if !unsigned && mode == 2 {
            return Err("unprivileged load/store unsupported".into());
        }
        let writeback = !unsigned && (mode == 1 || mode == 3);
        if writeback && src != 31 && src == dst {
            return Err("constrained-unpredictable memory writeback overlap".into());
        }
        let offset = if unsigned {
            (((word >> 10) & 4095) * u32::from(bytes)) as i64
        } else {
            signed((word >> 12) & 511, 9)
        };
        return Ok(Op::Memory {
            reg: dst,
            base: src,
            offset,
            bytes,
            load: opc == 1,
            writeback,
            post: !unsigned && mode == 1,
        });
    }
    if word & 0x3a000000 == 0x28000000 && word & (1 << 26) == 0 {
        let opc = word >> 30;
        let mode = (word >> 23) & 3;
        if (opc != 0 && opc != 2) || mode == 0 {
            return Err("unsupported pair addressing/width".into());
        }
        let bytes = if opc == 2 { 8 } else { 4 };
        let second = ((word >> 10) & 31) as u8;
        let load = word & (1 << 22) != 0;
        let writeback = mode != 2;
        if (load && dst == second) || (writeback && src != 31 && (src == dst || src == second)) {
            return Err("constrained-unpredictable pair overlap".into());
        }
        return Ok(Op::Pair {
            first: dst,
            second,
            base: src,
            offset: signed((word >> 15) & 127, 7) * i64::from(bytes),
            bytes,
            load,
            writeback,
            post: mode == 1,
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
