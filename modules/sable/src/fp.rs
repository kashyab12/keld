//! Bounded AArch64 FP/SIMD subset: V register state, FPCR/FPSR and the scalar
//! forms Xcode clang emits for CGFloat/CGRect code. See docs/fp-simd.md.
//!
//! Every other encoding in the FP/SIMD space is rejected with its opcode named.
pub mod soft;

use crate::{Result, engine::Cpu, memory::Memory};
use soft::{Format, Rounding};

/// FPSR bits that exist in AArch64: cumulative IOC/DZC/OFC/UFC/IXC, IDC and QC.
pub const FPSR_MASK: u64 = 0x0800_009f;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unary {
    Mov,
    Abs,
    Neg,
    Sqrt,
    Floor,
    Ceil,
    Trunc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binary {
    Mul,
    Div,
    Add,
    Sub,
    MaxNum,
    MinNum,
    NegMul,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SysReg {
    Fpcr,
    Fpsr,
}

/// Decoded FP/SIMD instruction. `double` selects D (true) or S (false) scalars.
/// Load/store `size` is log2 bytes: 0 = B, 1 = H, 2 = S, 3 = D, 4 = Q.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FpOp {
    Memory {
        rt: u8,
        base: u8,
        offset: i64,
        size: u8,
        load: bool,
        writeback: bool,
        post: bool,
    },
    MemoryReg {
        rt: u8,
        base: u8,
        index: u8,
        option: u8,
        scaled: bool,
        size: u8,
        load: bool,
    },
    Pair {
        first: u8,
        second: u8,
        base: u8,
        offset: i64,
        size: u8,
        load: bool,
        writeback: bool,
        post: bool,
    },
    Literal {
        rt: u8,
        address: u64,
        size: u8,
    },
    Unary {
        kind: Unary,
        dst: u8,
        src: u8,
        double: bool,
    },
    Binary {
        kind: Binary,
        dst: u8,
        lhs: u8,
        rhs: u8,
        double: bool,
    },
    /// FMADD/FMSUB/FNMADD/FNMSUB: dst = (-)addend + (-)lhs * rhs.
    MulAdd {
        dst: u8,
        lhs: u8,
        rhs: u8,
        addend: u8,
        negate_lhs: bool,
        negate_addend: bool,
        double: bool,
    },
    Immediate {
        dst: u8,
        bits: u64,
        double: bool,
    },
    /// FMOV Wd, Sn / Xd, Dn.
    ToGeneral {
        dst: u8,
        src: u8,
        wide: bool,
    },
    /// FMOV Sd, Wn / Dd, Xn.
    FromGeneral {
        dst: u8,
        src: u8,
        wide: bool,
    },
    /// MOV Vd.<8B|16B>, Vn (ORR with identical sources).
    VectorMove {
        dst: u8,
        src: u8,
        full: bool,
    },
    /// MOVI with a per-byte (op=1, cmode=1110) or replicated-byte (op=0) immediate.
    Movi {
        dst: u8,
        low: u64,
        high: u64,
    },
    Compare {
        lhs: u8,
        rhs: Option<u8>,
        signal_all: bool,
        double: bool,
    },
    Select {
        dst: u8,
        lhs: u8,
        rhs: u8,
        condition: u8,
        double: bool,
    },
    /// SCVTF/UCVTF from a general register.
    FromInt {
        dst: u8,
        src: u8,
        signed: bool,
        wide: bool,
        double: bool,
    },
    /// Scalar SCVTF/UCVTF with the integer already in Vn (same width).
    FromIntVector {
        dst: u8,
        src: u8,
        signed: bool,
        double: bool,
    },
    /// FCVTZS/FCVTZU to a general register.
    ToInt {
        dst: u8,
        src: u8,
        signed: bool,
        wide: bool,
        double: bool,
    },
    /// FCVT Dd, Sn (to_double) or FCVT Sd, Dn.
    Convert {
        dst: u8,
        src: u8,
        to_double: bool,
    },
    /// SXTL (SSHLL #0) from the low half: element 2 = 16->32 bits, 3 = 32->64 bits.
    SignExtend {
        dst: u8,
        src: u8,
        element: u8,
    },
    System {
        reg: SysReg,
        rt: u8,
        write: bool,
    },
}

fn signed(value: u32, bits: u32) -> i64 {
    ((value as i64) << (64 - bits)) >> (64 - bits)
}

fn reject(name: &str, word: u32, pc: u64) -> Result<FpOp> {
    Err(format!(
        "unsupported FP/SIMD instruction {name} {word:#010x} at {pc:#x}"
    ))
}

/// Decode a word in the FP/SIMD encoding space. `None` means the word is not an
/// FP/SIMD instruction and the integer decoder should handle it.
pub fn decode(word: u32, pc: u64) -> Option<Result<FpOp>> {
    let rd = (word & 31) as u8;
    // MRS/MSR FPCR/FPSR.
    if word & 0xffdf_ffc0 == 0xd51b_4400 {
        let reg = if word & 0x20 != 0 {
            SysReg::Fpsr
        } else {
            SysReg::Fpcr
        };
        return Some(Ok(FpOp::System {
            reg,
            rt: rd,
            write: word & (1 << 21) == 0,
        }));
    }
    // Loads and stores with V = 1 (op0 = x1x0, bit 26 set).
    if word & 0x0a00_0000 == 0x0800_0000 && word & (1 << 26) != 0 {
        return Some(decode_memory(word, pc));
    }
    // Data processing - scalar FP and Advanced SIMD (op0 = x111).
    if word & 0x0e00_0000 == 0x0e00_0000 {
        return Some(decode_data(word, pc));
    }
    None
}

fn decode_memory(word: u32, pc: u64) -> Result<FpOp> {
    let rt = (word & 31) as u8;
    let base = ((word >> 5) & 31) as u8;
    let size_field = word >> 30;
    let opc = (word >> 22) & 3;
    // Size of a single-register access: Q when size = 00 and opc<1> = 1.
    let single = |name: &str| -> Result<(u8, bool)> {
        match (size_field, opc) {
            (0, 2) => Ok((4, false)),
            (0, 3) => Ok((4, true)),
            (s, 0) => Ok((s as u8, false)),
            (s, 1) => Ok((s as u8, true)),
            _ => Err(format!(
                "unsupported FP/SIMD instruction {name} (unallocated size) {word:#010x} at {pc:#x}"
            )),
        }
    };
    if word & 0x3f00_0000 == 0x3d00_0000 {
        let (size, load) = single("LDR/STR (SIMD&FP, unsigned offset)")?;
        return Ok(FpOp::Memory {
            rt,
            base,
            offset: (((word >> 10) & 4095) as i64) << size,
            size,
            load,
            writeback: false,
            post: false,
        });
    }
    if word & 0x3f20_0000 == 0x3c00_0000 {
        let (size, load) = single("LDUR/STUR (SIMD&FP)")?;
        let mode = (word >> 10) & 3;
        if mode == 2 {
            return reject("LDTR/STTR (SIMD&FP, unallocated)", word, pc);
        }
        return Ok(FpOp::Memory {
            rt,
            base,
            offset: signed((word >> 12) & 511, 9),
            size,
            load,
            writeback: mode != 0,
            post: mode == 1,
        });
    }
    if word & 0x3f20_0c00 == 0x3c20_0800 {
        let (size, load) = single("LDR/STR (SIMD&FP, register)")?;
        let option = ((word >> 13) & 7) as u8;
        if option & 2 == 0 {
            return reject("LDR/STR (SIMD&FP, register, reserved extend)", word, pc);
        }
        return Ok(FpOp::MemoryReg {
            rt,
            base,
            index: ((word >> 16) & 31) as u8,
            option,
            scaled: word & (1 << 12) != 0,
            size,
            load,
        });
    }
    if word & 0x3c00_0000 == 0x2c00_0000 {
        let mode = (word >> 23) & 3;
        let size = match size_field {
            0 => 2,
            1 => 3,
            2 => 4,
            _ => return reject("LDP/STP (SIMD&FP, unallocated opc)", word, pc),
        };
        if mode == 0 {
            return reject("LDNP/STNP (SIMD&FP)", word, pc);
        }
        let second = ((word >> 10) & 31) as u8;
        let load = word & (1 << 22) != 0;
        if load && rt == second {
            return reject("LDP (SIMD&FP) with Rt == Rt2", word, pc);
        }
        return Ok(FpOp::Pair {
            first: rt,
            second,
            base,
            offset: signed((word >> 15) & 127, 7) << size,
            size,
            load,
            writeback: mode != 2,
            post: mode == 1,
        });
    }
    if word & 0x3f00_0000 == 0x1c00_0000 {
        let size = match size_field {
            0 => 2,
            1 => 3,
            2 => 4,
            _ => return reject("LDR (literal, SIMD&FP, unallocated opc)", word, pc),
        };
        return Ok(FpOp::Literal {
            rt,
            address: pc.wrapping_add_signed(signed((word >> 5) & 0x7ffff, 19) << 2),
            size,
        });
    }
    let name = if word & 0xbf00_0000 == 0x0c00_0000 || word & 0xbf80_0000 == 0x0d00_0000 {
        "LD1/ST1-family structure load/store"
    } else if word & 0x3f20_0c00 == 0x3c20_0000 {
        "atomic memory operation (SIMD&FP space)"
    } else {
        "SIMD&FP load/store (other)"
    };
    reject(name, word, pc)
}

fn scalar_name(word: u32) -> &'static str {
    let ftype = (word >> 22) & 3;
    if ftype == 3 {
        return "half-precision scalar FP";
    }
    if ftype == 2 {
        return "scalar FP (unallocated ftype)";
    }
    if word & 0xff20_7c00 == 0x1e20_4000 {
        return match (word >> 15) & 63 {
            0 => "FMOV (register)",
            1 => "FABS",
            2 => "FNEG",
            3 => "FSQRT",
            4 | 5 => "FCVT",
            7 => "FCVT (to half precision)",
            8 => "FRINTN",
            9 => "FRINTP",
            10 => "FRINTM",
            11 => "FRINTZ",
            12 => "FRINTA",
            14 => "FRINTX",
            15 => "FRINTI",
            16..=19 => "FRINT32Z/FRINT32X/FRINT64Z/FRINT64X",
            _ => "scalar FP data-processing (1 source, unallocated)",
        };
    }
    if word & 0xff20_3c00 == 0x1e20_2000 {
        return "FCMP/FCMPE";
    }
    if word & 0xff20_1c00 == 0x1e20_1000 {
        return "FMOV (scalar, immediate)";
    }
    if word & 0xff20_0c00 == 0x1e20_0400 {
        return if word & 0x10 != 0 { "FCCMPE" } else { "FCCMP" };
    }
    if word & 0xff20_0c00 == 0x1e20_0800 {
        return match (word >> 12) & 15 {
            0 => "FMUL",
            1 => "FDIV",
            2 => "FADD",
            3 => "FSUB",
            4 => "FMAX",
            5 => "FMIN",
            6 => "FMAXNM",
            7 => "FMINNM",
            8 => "FNMUL",
            _ => "scalar FP data-processing (2 source, unallocated)",
        };
    }
    if word & 0xff20_0c00 == 0x1e20_0c00 {
        return "FCSEL";
    }
    "scalar FP (other)"
}

fn conversion_name(word: u32) -> &'static str {
    match ((word >> 19) & 3, (word >> 16) & 7) {
        (0, 0) => "FCVTNS",
        (0, 1) => "FCVTNU",
        (0, 2) => "SCVTF (scalar, integer)",
        (0, 3) => "UCVTF (scalar, integer)",
        (0, 4) => "FCVTAS",
        (0, 5) => "FCVTAU",
        (0, 6) | (0, 7) => "FMOV (general)",
        (1, 0) => "FCVTPS",
        (1, 1) => "FCVTPU",
        (1, 6) | (1, 7) => "FMOV (general, top half of 128-bit)",
        (2, 0) => "FCVTMS",
        (2, 1) => "FCVTMU",
        (3, 0) => "FCVTZS (scalar, integer)",
        (3, 1) => "FCVTZU (scalar, integer)",
        (3, 6) => "FJCVTZS",
        _ => "FP<->integer conversion (unallocated)",
    }
}

fn simd_name(word: u32) -> &'static str {
    if word & 0x9f20_0400 == 0x0e20_0400 {
        return "Advanced SIMD three same";
    }
    if word & 0x9f20_0c00 == 0x0e20_0000 {
        return "Advanced SIMD three different";
    }
    if word & 0x9f3e_0c00 == 0x0e20_0800 {
        return "Advanced SIMD two-register miscellaneous";
    }
    if word & 0x9f3e_0c00 == 0x0e30_0800 {
        return "Advanced SIMD across lanes";
    }
    if word & 0x9fe0_8400 == 0x0e00_0400 {
        return "Advanced SIMD copy (DUP/INS/UMOV/SMOV)";
    }
    if word & 0x9ff8_0400 == 0x0f00_0400 {
        return "Advanced SIMD modified immediate (MOVI/MVNI/ORR/BIC/FMOV vector)";
    }
    if word & 0x9f80_0400 == 0x0f00_0400 {
        return "Advanced SIMD shift by immediate";
    }
    if word & 0x9f00_0400 == 0x0f00_0000 {
        return "Advanced SIMD vector x indexed element";
    }
    if word & 0xbf20_8c00 == 0x0e00_0000 {
        return "Advanced SIMD table lookup (TBL/TBX)";
    }
    if word & 0xbf20_8c00 == 0x0e00_0800 {
        return "Advanced SIMD permute (UZP/TRN/ZIP)";
    }
    if word & 0xbf20_8400 == 0x2e00_0000 {
        return "Advanced SIMD extract (EXT)";
    }
    if word & 0xdf00_0000 == 0x5e00_0000 || word & 0xdf00_0000 == 0x5f00_0000 {
        return "Advanced SIMD scalar";
    }
    if word & 0xff00_0000 == 0xce00_0000 {
        return "cryptographic extension";
    }
    "Advanced SIMD (other)"
}

fn decode_data(word: u32, pc: u64) -> Result<FpOp> {
    let rd = (word & 31) as u8;
    let rn = ((word >> 5) & 31) as u8;
    let rm = ((word >> 16) & 31) as u8;
    let ftype = (word >> 22) & 3;
    let double = ftype == 1;
    // Conversions between FP and general registers (bit 31 is sf here).
    if word & 0x7f20_fc00 == 0x1e20_0000 {
        if ftype > 1 {
            return reject(conversion_name(word), word, pc);
        }
        let wide = word >> 31 != 0;
        return match ((word >> 19) & 3, (word >> 16) & 7) {
            (0, kind @ (2 | 3)) => Ok(FpOp::FromInt {
                dst: rd,
                src: rn,
                signed: kind == 2,
                wide,
                double,
            }),
            (3, kind @ (0 | 1)) => Ok(FpOp::ToInt {
                dst: rd,
                src: rn,
                signed: kind == 0,
                wide,
                double,
            }),
            (0, kind @ (6 | 7)) if wide == double => {
                if kind == 6 {
                    Ok(FpOp::ToGeneral {
                        dst: rd,
                        src: rn,
                        wide,
                    })
                } else {
                    Ok(FpOp::FromGeneral {
                        dst: rd,
                        src: rn,
                        wide,
                    })
                }
            }
            _ => reject(conversion_name(word), word, pc),
        };
    }
    if word & 0x7f20_0000 == 0x1e00_0000 {
        return reject("SCVTF/UCVTF/FCVTZS/FCVTZU (fixed-point)", word, pc);
    }
    // Scalar FP: M = 0, S = 0, bits 28:24 = 11110, bit 21 = 1.
    if word & 0xff20_0000 == 0x1e20_0000 {
        if ftype > 1 {
            return reject(scalar_name(word), word, pc);
        }
        if word & 0xff20_7c00 == 0x1e20_4000 {
            let opcode = (word >> 15) & 63;
            let unary = |kind| {
                Ok(FpOp::Unary {
                    kind,
                    dst: rd,
                    src: rn,
                    double,
                })
            };
            return match opcode {
                0 => unary(Unary::Mov),
                1 => unary(Unary::Abs),
                2 => unary(Unary::Neg),
                3 => unary(Unary::Sqrt),
                4 | 5 if (opcode == 5) != double => Ok(FpOp::Convert {
                    dst: rd,
                    src: rn,
                    to_double: opcode == 5,
                }),
                9 => unary(Unary::Ceil),
                10 => unary(Unary::Floor),
                11 => unary(Unary::Trunc),
                _ => reject(scalar_name(word), word, pc),
            };
        }
        if word & 0xff20_3c00 == 0x1e20_2000 {
            let op = (word >> 14) & 3;
            let opcode2 = word & 31;
            if op != 0 || opcode2 & 0b00111 != 0 {
                return reject("FCMP/FCMPE (unallocated)", word, pc);
            }
            let zero = opcode2 & 8 != 0;
            if zero && rm != 0 {
                return reject("FCMP/FCMPE #0.0 with nonzero Rm", word, pc);
            }
            return Ok(FpOp::Compare {
                lhs: rn,
                rhs: (!zero).then_some(rm),
                signal_all: opcode2 & 16 != 0,
                double,
            });
        }
        if word & 0xff20_1fe0 == 0x1e20_1000 {
            let format = Format::of(double);
            return Ok(FpOp::Immediate {
                dst: rd,
                bits: format.expand_imm(((word >> 13) & 255) as u8),
                double,
            });
        }
        if word & 0xff20_0c00 == 0x1e20_0800 {
            let kind = match (word >> 12) & 15 {
                0 => Binary::Mul,
                1 => Binary::Div,
                2 => Binary::Add,
                3 => Binary::Sub,
                6 => Binary::MaxNum,
                7 => Binary::MinNum,
                8 => Binary::NegMul,
                _ => return reject(scalar_name(word), word, pc),
            };
            return Ok(FpOp::Binary {
                kind,
                dst: rd,
                lhs: rn,
                rhs: rm,
                double,
            });
        }
        if word & 0xff20_0c00 == 0x1e20_0c00 {
            return Ok(FpOp::Select {
                dst: rd,
                lhs: rn,
                rhs: rm,
                condition: ((word >> 12) & 15) as u8,
                double,
            });
        }
        return reject(scalar_name(word), word, pc);
    }
    // FP data-processing (3 source).
    if word & 0xff00_0000 == 0x1f00_0000 {
        if ftype > 1 {
            return reject("half-precision FMADD/FMSUB/FNMADD/FNMSUB", word, pc);
        }
        let o1 = word & (1 << 21) != 0;
        let o0 = word & (1 << 15) != 0;
        return Ok(FpOp::MulAdd {
            dst: rd,
            lhs: rn,
            rhs: rm,
            addend: ((word >> 10) & 31) as u8,
            negate_lhs: o0 != o1,
            negate_addend: o1,
            double,
        });
    }
    if word & 0x5f00_0000 == 0x1e00_0000 || word & 0x5f00_0000 == 0x1f00_0000 {
        return reject("scalar FP (unallocated M/S)", word, pc);
    }
    // Advanced SIMD modified immediate: only the byte-replicate/byte-mask MOVI forms.
    if word & 0x9ff8_0400 == 0x0f00_0400 {
        let q = word & (1 << 30) != 0;
        let op = word & (1 << 29) != 0;
        let cmode = (word >> 12) & 15;
        let o2 = word & (1 << 11) != 0;
        if cmode == 0b1110 && !o2 {
            let imm8 = (((word >> 16) & 7) << 5 | ((word >> 5) & 31)) as u8;
            let value = if op {
                (0..8).fold(0u64, |v, i| {
                    v | if imm8 >> i & 1 != 0 {
                        0xffu64 << (i * 8)
                    } else {
                        0
                    }
                })
            } else {
                u64::from(imm8) * 0x0101_0101_0101_0101
            };
            return Ok(FpOp::Movi {
                dst: rd,
                low: value,
                high: if q { value } else { 0 },
            });
        }
        let name = match (op, cmode) {
            (_, 15) => "FMOV (vector, immediate)",
            (false, c) if c & 9 == 1 || c & 13 == 9 => "ORR (vector, immediate)",
            (true, c) if c & 9 == 1 || c & 13 == 9 => "BIC (vector, immediate)",
            (false, _) => "MOVI (unsupported shifted form)",
            (true, _) => "MVNI",
        };
        return reject(name, word, pc);
    }
    // MOV Vd.T, Vn.T: ORR (vector, register) with Rm == Rn.
    if word & 0xbfe0_fc00 == 0x0ea0_1c00 {
        if rm != rn {
            return reject("ORR (vector, register)", word, pc);
        }
        return Ok(FpOp::VectorMove {
            dst: rd,
            src: rn,
            full: word & (1 << 30) != 0,
        });
    }
    // SSHLL #0 (SXTL) on the lower half, 16->32 or 32->64 bits.
    if word & 0xff80_fc00 == 0x0f00_a400 && (word >> 19) & 15 != 0 {
        let immh = (word >> 19) & 15;
        let shift_field = (word >> 16) & 127;
        let element = match immh {
            0b0010 | 0b0011 if shift_field == 16 => 2,
            0b0100..=0b0111 if shift_field == 32 => 3,
            _ => return reject("SSHLL/SXTL (unsupported element size or shift)", word, pc),
        };
        return Ok(FpOp::SignExtend {
            dst: rd,
            src: rn,
            element,
        });
    }
    if word & 0xbf80_fc00 == 0x0f00_a400 || word & 0xbf80_fc00 == 0x2f00_a400 {
        return reject(
            if word & (1 << 29) != 0 {
                "USHLL/UXTL"
            } else {
                "SSHLL2/SXTL2"
            },
            word,
            pc,
        );
    }
    // Scalar SCVTF/UCVTF (vector register holds the same-width integer).
    if word & 0xdfbf_fc00 == 0x5e21_d800 {
        return Ok(FpOp::FromIntVector {
            dst: rd,
            src: rn,
            signed: word & (1 << 29) == 0,
            double: word & (1 << 22) != 0,
        });
    }
    reject(simd_name(word), word, pc)
}

fn element(cpu: &Cpu, n: u8, double: bool) -> u64 {
    let value = cpu.v[n as usize] as u64;
    if double { value } else { value & 0xffff_ffff }
}

/// Scalar writes zero the rest of the 128-bit register.
fn set_scalar(cpu: &mut Cpu, n: u8, value: u64, double: bool) {
    cpu.v[n as usize] = u128::from(if double { value } else { value & 0xffff_ffff });
}

fn bytes_mask(size: u8) -> u128 {
    if size == 4 {
        u128::MAX
    } else {
        (1u128 << (8u32 << size)) - 1
    }
}

fn load_v(cpu: &mut Cpu, rt: u8, data: &[u8]) {
    let mut value = [0u8; 16];
    value[..data.len()].copy_from_slice(data);
    cpu.v[rt as usize] = u128::from_le_bytes(value);
}

fn base_address(cpu: &Cpu, base: u8) -> Result<u64> {
    let original = cpu.reg(base, true);
    if base == 31 && !original.is_multiple_of(16) {
        return Err("unaligned guest SP".into());
    }
    Ok(original)
}

/// Execute one decoded FP/SIMD instruction. Errors leave the CPU unchanged.
pub fn execute(cpu: &mut Cpu, op: FpOp, memory: Option<&mut Memory>, pc: u64) -> Result<()> {
    let mut flags = 0u32;
    match op {
        FpOp::Memory {
            rt,
            base,
            offset,
            size,
            load,
            writeback,
            post,
        } => {
            let memory = memory.ok_or("memory operation requires per-run memory")?;
            let original = base_address(cpu, base)?;
            let adjusted = original.wrapping_add_signed(offset);
            let address = if post { original } else { adjusted };
            access(cpu, memory, rt, address, size, load)?;
            if writeback {
                cpu.set_sp_or_x(base, adjusted);
            }
        }
        FpOp::MemoryReg {
            rt,
            base,
            index,
            option,
            scaled,
            size,
            load,
        } => {
            let memory = memory.ok_or("memory operation requires per-run memory")?;
            let raw = cpu.reg(index, false);
            let offset = match option {
                2 => raw as u32 as u64,
                6 => raw as i32 as i64 as u64,
                _ => raw,
            };
            let offset = offset.wrapping_shl(if scaled { u32::from(size) } else { 0 });
            let address = base_address(cpu, base)?.wrapping_add(offset);
            access(cpu, memory, rt, address, size, load)?;
        }
        FpOp::Pair {
            first,
            second,
            base,
            offset,
            size,
            load,
            writeback,
            post,
        } => {
            let memory = memory.ok_or("memory operation requires per-run memory")?;
            let original = base_address(cpu, base)?;
            let adjusted = original.wrapping_add_signed(offset);
            let address = if post { original } else { adjusted };
            let width = 1usize << size;
            if load {
                let data = memory.read(address, width * 2)?.to_vec();
                load_v(cpu, first, &data[..width]);
                load_v(cpu, second, &data[width..]);
            } else {
                let mut data = Vec::with_capacity(width * 2);
                data.extend_from_slice(&cpu.v[first as usize].to_le_bytes()[..width]);
                data.extend_from_slice(&cpu.v[second as usize].to_le_bytes()[..width]);
                memory.write(address, &data)?;
            }
            if writeback {
                cpu.set_sp_or_x(base, adjusted);
            }
        }
        FpOp::Literal { rt, address, size } => {
            let memory = memory.ok_or("literal load requires per-run memory")?;
            let data = memory.read(address, 1 << size)?.to_vec();
            load_v(cpu, rt, &data);
        }
        FpOp::Unary {
            kind,
            dst,
            src,
            double,
        } => {
            let f = Format::of(double);
            let a = element(cpu, src, double);
            let result = match kind {
                Unary::Mov => a,
                Unary::Abs => soft::abs(f, a),
                Unary::Neg => soft::neg(f, a),
                Unary::Sqrt => soft::sqrt(f, a, &mut flags),
                Unary::Floor => soft::round_int(f, a, Rounding::Floor, &mut flags),
                Unary::Ceil => soft::round_int(f, a, Rounding::Ceil, &mut flags),
                Unary::Trunc => soft::round_int(f, a, Rounding::Trunc, &mut flags),
            };
            set_scalar(cpu, dst, result, double);
        }
        FpOp::Binary {
            kind,
            dst,
            lhs,
            rhs,
            double,
        } => {
            let f = Format::of(double);
            let a = element(cpu, lhs, double);
            let b = element(cpu, rhs, double);
            let result = match kind {
                Binary::Add => soft::add(f, a, b, false, &mut flags),
                Binary::Sub => soft::add(f, a, b, true, &mut flags),
                Binary::Mul => soft::mul(f, a, b, &mut flags),
                Binary::NegMul => soft::neg(f, soft::mul(f, a, b, &mut flags)),
                Binary::Div => soft::div(f, a, b, &mut flags),
                Binary::MaxNum => soft::min_max_num(f, a, b, true, &mut flags),
                Binary::MinNum => soft::min_max_num(f, a, b, false, &mut flags),
            };
            set_scalar(cpu, dst, result, double);
        }
        FpOp::MulAdd {
            dst,
            lhs,
            rhs,
            addend,
            negate_lhs,
            negate_addend,
            double,
        } => {
            let f = Format::of(double);
            let mut a = element(cpu, lhs, double);
            let b = element(cpu, rhs, double);
            let mut c = element(cpu, addend, double);
            if negate_lhs {
                a = soft::neg(f, a);
            }
            if negate_addend {
                c = soft::neg(f, c);
            }
            let result = soft::mul_add(f, c, a, b, &mut flags);
            set_scalar(cpu, dst, result, double);
        }
        FpOp::Immediate { dst, bits, double } => set_scalar(cpu, dst, bits, double),
        FpOp::ToGeneral { dst, src, wide } => {
            let value = element(cpu, src, wide);
            cpu.set_x(dst, value);
        }
        FpOp::FromGeneral { dst, src, wide } => {
            let value = cpu.reg(src, false);
            set_scalar(cpu, dst, value, wide);
        }
        FpOp::VectorMove { dst, src, full } => {
            let value = cpu.v[src as usize];
            cpu.v[dst as usize] = if full { value } else { value & bytes_mask(3) };
        }
        FpOp::Movi { dst, low, high } => {
            cpu.v[dst as usize] = u128::from(low) | (u128::from(high) << 64);
        }
        FpOp::Compare {
            lhs,
            rhs,
            signal_all,
            double,
        } => {
            let f = Format::of(double);
            let a = element(cpu, lhs, double);
            let b = rhs.map_or(0, |r| element(cpu, r, double));
            cpu.nzcv = soft::fcmp(f, a, b, signal_all, &mut flags);
        }
        FpOp::Select {
            dst,
            lhs,
            rhs,
            condition,
            double,
        } => {
            let pick = if crate::engine::condition_holds(cpu.nzcv, condition) {
                lhs
            } else {
                rhs
            };
            let value = element(cpu, pick, double);
            set_scalar(cpu, dst, value, double);
        }
        FpOp::FromInt {
            dst,
            src,
            signed,
            wide,
            double,
        } => {
            let value = cpu.reg(src, false);
            let result = soft::from_int(
                Format::of(double),
                value,
                signed,
                if wide { 64 } else { 32 },
                &mut flags,
            );
            set_scalar(cpu, dst, result, double);
        }
        FpOp::FromIntVector {
            dst,
            src,
            signed,
            double,
        } => {
            let value = element(cpu, src, double);
            let result = soft::from_int(
                Format::of(double),
                value,
                signed,
                if double { 64 } else { 32 },
                &mut flags,
            );
            set_scalar(cpu, dst, result, double);
        }
        FpOp::ToInt {
            dst,
            src,
            signed,
            wide,
            double,
        } => {
            let value = element(cpu, src, double);
            let result = soft::to_int(
                Format::of(double),
                value,
                signed,
                if wide { 64 } else { 32 },
                &mut flags,
            );
            cpu.set_x(dst, result);
        }
        FpOp::Convert {
            dst,
            src,
            to_double,
        } => {
            let from = Format::of(!to_double);
            let to = Format::of(to_double);
            let value = element(cpu, src, !to_double);
            let result = soft::convert(from, to, value, &mut flags);
            set_scalar(cpu, dst, result, to_double);
        }
        FpOp::SignExtend { dst, src, element } => {
            let value = cpu.v[src as usize] as u64;
            cpu.v[dst as usize] = if element == 3 {
                let lo = value as u32 as i32 as i64 as u64 as u128;
                let hi = (value >> 32) as u32 as i32 as i64 as u64 as u128;
                lo | (hi << 64)
            } else {
                (0..4).fold(0u128, |v, i| {
                    let lane = (value >> (i * 16)) as u16 as i16 as i32 as u32;
                    v | (u128::from(lane) << (i * 32))
                })
            };
        }
        FpOp::System { reg, rt, write } => match (reg, write) {
            (SysReg::Fpcr, true) => {
                let value = cpu.reg(rt, false);
                if value != 0 {
                    return Err(format!(
                        "MSR FPCR {value:#x} rejected at {pc:#x}: only RN rounding with FZ=0, DN=0, AHP=0 and no trap enables (FPCR=0) is supported"
                    ));
                }
                cpu.fpcr = 0;
            }
            (SysReg::Fpsr, true) => cpu.fpsr = cpu.reg(rt, false) & FPSR_MASK,
            (SysReg::Fpcr, false) => cpu.set_x(rt, cpu.fpcr),
            (SysReg::Fpsr, false) => cpu.set_x(rt, cpu.fpsr),
        },
    }
    cpu.fpsr |= u64::from(flags);
    Ok(())
}

fn access(
    cpu: &mut Cpu,
    memory: &mut Memory,
    rt: u8,
    address: u64,
    size: u8,
    load: bool,
) -> Result<()> {
    let width = 1usize << size;
    if load {
        let data = memory.read(address, width)?.to_vec();
        load_v(cpu, rt, &data);
    } else {
        memory.write(address, &cpu.v[rt as usize].to_le_bytes()[..width])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn op(word: u32) -> FpOp {
        decode(word, 0x1000).unwrap().unwrap()
    }
    fn rejected(word: u32) -> String {
        decode(word, 0x1000).unwrap().unwrap_err()
    }
    #[test]
    fn decodes_compiler_emitted_forms() {
        assert_eq!(
            op(0x6f00_e400),
            FpOp::Movi {
                dst: 0,
                low: 0,
                high: 0
            }
        );
        assert!(matches!(
            op(0x4ea1_1c20),
            FpOp::VectorMove {
                dst: 0,
                src: 1,
                full: true
            }
        ));
        assert!(matches!(
            op(0x1e60_2000),
            FpOp::Compare { rhs: Some(0), .. }
        ));
        assert!(matches!(op(0x1e60_2008), FpOp::Compare { rhs: None, .. }));
        assert!(matches!(
            op(0x9e67_0000),
            FpOp::FromGeneral { wide: true, .. }
        ));
        assert!(matches!(
            op(0x9e66_0000),
            FpOp::ToGeneral { wide: true, .. }
        ));
        assert!(matches!(
            op(0x1f42_0c20),
            FpOp::MulAdd {
                negate_lhs: false,
                negate_addend: false,
                ..
            }
        ));
        assert!(matches!(
            op(0x0f20_a400),
            FpOp::SignExtend { element: 3, .. }
        ));
        assert!(matches!(
            op(0x0f10_a400),
            FpOp::SignExtend { element: 2, .. }
        ));
        assert!(matches!(
            op(0x5e61_d800),
            FpOp::FromIntVector {
                signed: true,
                double: true,
                ..
            }
        ));
        assert!(matches!(
            op(0xad00_07e0),
            FpOp::Pair {
                size: 4,
                load: false,
                writeback: false,
                ..
            }
        ));
        assert_eq!(
            op(0x1e6e_1000),
            FpOp::Immediate {
                dst: 0,
                bits: 1f64.to_bits(),
                double: true
            }
        );
    }
    #[test]
    fn unsupported_forms_are_named() {
        for (word, name) in [
            (0x1e60_4800, "FMAX"),
            (0x1e60_5800, "FMIN"),
            (0x1e66_4000, "FRINTA"),
            (0x1e67_4000, "FRINTX"),
            (0x1e64_4000, "FRINTN"),
            (0x1ee2_2800, "half-precision"),
            (0x4e20_8400, "Advanced SIMD three same"),
            (0x4c40_7000, "LD1/ST1"),
            (0x1e62_0400, "FCCMP"),
            (0x9e58_0000, "fixed-point"),
            (0x1e24_0000, "FCVTAS"),
            (0x4f00_0420, "MOVI (unsupported shifted form)"),
            (0x2f00_a400, "MVNI"),
            (0x2f20_a400, "USHLL"),
            (0x4ea2_1c20, "ORR (vector, register)"),
            (0x6c00_07e0, "LDNP/STNP"),
            (0x4e08_1c00, "Advanced SIMD copy"),
        ] {
            let message = rejected(word);
            assert!(message.contains(name), "{word:#x}: {message}");
            assert!(message.contains("unsupported FP/SIMD instruction"));
        }
    }
}
