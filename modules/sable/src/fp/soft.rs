//! Bit-exact IEEE 754 binary32/binary64 arithmetic with the AArch64 rules Sable
//! supports: FPCR = 0 (round to nearest even, FZ = 0, DN = 0, no traps).
//!
//! Nothing here uses host floating point. NaN selection follows the ARM
//! pseudocode (FPProcessNaN/FPProcessNaNs/FPProcessNaNs3), tininess is detected
//! before rounding, and every function returns the FPSR cumulative flags it raises.

pub const IOC: u32 = 1;
pub const DZC: u32 = 1 << 1;
pub const OFC: u32 = 1 << 2;
pub const UFC: u32 = 1 << 3;
pub const IXC: u32 = 1 << 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    exp_bits: u32,
    frac_bits: u32,
}
pub const F32: Format = Format {
    exp_bits: 8,
    frac_bits: 23,
};
pub const F64: Format = Format {
    exp_bits: 11,
    frac_bits: 52,
};

impl Format {
    pub fn of(double: bool) -> Self {
        if double { F64 } else { F32 }
    }
    fn bias(self) -> i32 {
        (1 << (self.exp_bits - 1)) - 1
    }
    fn max_field(self) -> u64 {
        (1 << self.exp_bits) - 1
    }
    pub fn sign_bit(self) -> u64 {
        1 << (self.exp_bits + self.frac_bits)
    }
    fn frac_mask(self) -> u64 {
        (1 << self.frac_bits) - 1
    }
    fn quiet_bit(self) -> u64 {
        1 << (self.frac_bits - 1)
    }
    pub fn mask(self) -> u64 {
        (self.sign_bit() << 1).wrapping_sub(1)
    }
    pub fn default_nan(self) -> u64 {
        (self.max_field() << self.frac_bits) | self.quiet_bit()
    }
    fn infinity(self, sign: bool) -> u64 {
        (self.max_field() << self.frac_bits) | if sign { self.sign_bit() } else { 0 }
    }
    fn zero(self, sign: bool) -> u64 {
        if sign { self.sign_bit() } else { 0 }
    }
    /// VFPExpandImm for FMOV (scalar, immediate).
    pub fn expand_imm(self, imm8: u8) -> u64 {
        let sign = u64::from(imm8 >> 7);
        let b6 = u64::from((imm8 >> 6) & 1);
        let e = self.exp_bits;
        let exp = ((b6 ^ 1) << (e - 1))
            | (if b6 == 1 { (1 << (e - 3)) - 1 } else { 0 } << 2)
            | u64::from((imm8 >> 4) & 3);
        let frac = u64::from(imm8 & 15) << (self.frac_bits - 4);
        (sign << (e + self.frac_bits)) | (exp << self.frac_bits) | frac
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Zero,
    Finite,
    Infinity,
    Quiet,
    Signalling,
}

/// value = (-1)^sign * sig * 2^exp for finite numbers.
#[derive(Clone, Copy, Debug)]
struct Unpacked {
    sign: bool,
    kind: Kind,
    exp: i32,
    sig: u64,
}

fn unpack(f: Format, bits: u64) -> Unpacked {
    let sign = bits & f.sign_bit() != 0;
    let field = (bits >> f.frac_bits) & f.max_field();
    let frac = bits & f.frac_mask();
    let min_exp = 1 - f.bias() - f.frac_bits as i32;
    let (kind, exp, sig) = if field == f.max_field() {
        if frac == 0 {
            (Kind::Infinity, 0, 0)
        } else if frac & f.quiet_bit() != 0 {
            (Kind::Quiet, 0, 0)
        } else {
            (Kind::Signalling, 0, 0)
        }
    } else if field == 0 {
        if frac == 0 {
            (Kind::Zero, 0, 0)
        } else {
            (Kind::Finite, min_exp, frac)
        }
    } else {
        (
            Kind::Finite,
            field as i32 + min_exp - 1,
            frac | (1 << f.frac_bits),
        )
    };
    Unpacked {
        sign,
        kind,
        exp,
        sig,
    }
}

fn is_nan(u: &Unpacked) -> bool {
    matches!(u.kind, Kind::Quiet | Kind::Signalling)
}

/// Shift right, OR-ing every discarded bit into bit 0.
fn jam(sig: u128, shift: u32) -> u128 {
    if shift == 0 {
        sig
    } else if shift >= 128 {
        u128::from(sig != 0)
    } else {
        (sig >> shift) | u128::from(sig & ((1u128 << shift) - 1) != 0)
    }
}

/// FPRound for RN with FZ = 0: the exact value is sig * 2^exp (sig > 0).
fn round(f: Format, sign: bool, exp: i32, sig: u128, flags: &mut u32) -> u64 {
    debug_assert!(sig != 0);
    let p = f.frac_bits as i32 + 1;
    // Keep p significant bits, one round bit and one sticky bit.
    let want = p + 1;
    let msb = 127 - sig.leading_zeros() as i32;
    let (mut sig, exp) = if msb > want {
        (jam(sig, (msb - want) as u32), exp + msb - want)
    } else {
        (sig << (want - msb), exp - (want - msb))
    };
    let mut biased = exp + want + f.bias();
    let tiny = biased < 1;
    if tiny {
        sig = jam(sig, (1 - biased).min(200) as u32);
    }
    let low = sig & 3;
    let mut mant = sig >> 2;
    if low & 2 != 0 && (low & 1 != 0 || mant & 1 != 0) {
        mant += 1;
    }
    let inexact = low != 0;
    let sign_bits = f.zero(sign);
    if tiny {
        if inexact {
            *flags |= UFC | IXC;
        }
        return sign_bits | mant as u64;
    }
    if mant >> p != 0 {
        mant >>= 1;
        biased += 1;
    }
    if biased as u64 >= f.max_field() {
        *flags |= OFC | IXC;
        return f.infinity(sign);
    }
    if inexact {
        *flags |= IXC;
    }
    sign_bits | ((biased as u64) << f.frac_bits) | (mant as u64 & f.frac_mask())
}

fn quiet(f: Format, bits: u64) -> u64 {
    bits | f.quiet_bit()
}

/// FPProcessNaN for one operand.
fn process_nan(f: Format, bits: u64, u: &Unpacked, flags: &mut u32) -> u64 {
    if u.kind == Kind::Signalling {
        *flags |= IOC;
    }
    quiet(f, bits)
}

/// FPProcessNaNs: signalling NaNs first in operand order, then quiet NaNs.
fn process_nans(f: Format, ops: &[(u64, Unpacked)], flags: &mut u32) -> Option<u64> {
    ops.iter()
        .find(|(_, u)| u.kind == Kind::Signalling)
        .or_else(|| ops.iter().find(|(_, u)| u.kind == Kind::Quiet))
        .map(|(bits, u)| process_nan(f, *bits, u, flags))
}

pub fn neg(f: Format, a: u64) -> u64 {
    (a ^ f.sign_bit()) & f.mask()
}
pub fn abs(f: Format, a: u64) -> u64 {
    a & !f.sign_bit() & f.mask()
}

pub fn add(f: Format, a: u64, b: u64, subtract: bool, flags: &mut u32) -> u64 {
    let x = unpack(f, a);
    let mut y = unpack(f, b);
    if let Some(nan) = process_nans(f, &[(a, x), (b, y)], flags) {
        return nan;
    }
    y.sign ^= subtract;
    match (x.kind, y.kind) {
        (Kind::Infinity, Kind::Infinity) if x.sign != y.sign => {
            *flags |= IOC;
            f.default_nan()
        }
        (Kind::Infinity, _) => f.infinity(x.sign),
        (_, Kind::Infinity) => f.infinity(y.sign),
        (Kind::Zero, Kind::Zero) => f.zero(x.sign && y.sign),
        _ => {
            let (mut sx, mut sy) = ((x.sig as u128) << 64, (y.sig as u128) << 64);
            let exp = match (x.kind, y.kind) {
                (Kind::Zero, _) => y.exp,
                (_, Kind::Zero) => x.exp,
                _ => x.exp.max(y.exp),
            };
            if x.kind != Kind::Zero {
                sx = jam(sx, (exp - x.exp) as u32);
            }
            if y.kind != Kind::Zero {
                sy = jam(sy, (exp - y.exp) as u32);
            }
            let (sign, sum) = if x.sign == y.sign {
                (x.sign, sx + sy)
            } else if sx >= sy {
                (x.sign, sx - sy)
            } else {
                (y.sign, sy - sx)
            };
            if sum == 0 {
                return f.zero(false);
            }
            round(f, sign, exp - 64, sum, flags)
        }
    }
}

pub fn mul(f: Format, a: u64, b: u64, flags: &mut u32) -> u64 {
    let x = unpack(f, a);
    let y = unpack(f, b);
    if let Some(nan) = process_nans(f, &[(a, x), (b, y)], flags) {
        return nan;
    }
    let sign = x.sign != y.sign;
    match (x.kind, y.kind) {
        (Kind::Infinity, Kind::Zero) | (Kind::Zero, Kind::Infinity) => {
            *flags |= IOC;
            f.default_nan()
        }
        (Kind::Infinity, _) | (_, Kind::Infinity) => f.infinity(sign),
        (Kind::Zero, _) | (_, Kind::Zero) => f.zero(sign),
        _ => round(f, sign, x.exp + y.exp, x.sig as u128 * y.sig as u128, flags),
    }
}

/// FPMulAdd(addend, op1, op2): addend + op1 * op2 with a single rounding.
pub fn mul_add(f: Format, addend: u64, a: u64, b: u64, flags: &mut u32) -> u64 {
    let c = unpack(f, addend);
    let x = unpack(f, a);
    let y = unpack(f, b);
    let inf_zero = matches!(
        (x.kind, y.kind),
        (Kind::Infinity, Kind::Zero) | (Kind::Zero, Kind::Infinity)
    );
    if let Some(nan) = process_nans(f, &[(addend, c), (a, x), (b, y)], flags) {
        if c.kind == Kind::Quiet && inf_zero {
            *flags |= IOC;
            return f.default_nan();
        }
        return nan;
    }
    let product_sign = x.sign != y.sign;
    let product_inf = x.kind == Kind::Infinity || y.kind == Kind::Infinity;
    let product_zero = x.kind == Kind::Zero || y.kind == Kind::Zero;
    let addend_inf = c.kind == Kind::Infinity;
    if inf_zero || (addend_inf && product_inf && c.sign != product_sign) {
        *flags |= IOC;
        return f.default_nan();
    }
    if addend_inf {
        return f.infinity(c.sign);
    }
    if product_inf {
        return f.infinity(product_sign);
    }
    let addend_zero = c.kind == Kind::Zero;
    if addend_zero && product_zero {
        return f.zero(c.sign && product_sign);
    }
    if product_zero {
        return addend & f.mask();
    }
    let mut sp = x.sig as u128 * y.sig as u128;
    let mut ep = x.exp + y.exp;
    if addend_zero {
        return round(f, product_sign, ep, sp, flags);
    }
    // Normalise both exact operands to bit 125, then align with a sticky bit.
    let mut sa = c.sig as u128;
    let mut ea = c.exp;
    let lp = sp.leading_zeros() - 2;
    sp <<= lp;
    ep -= lp as i32;
    let la = sa.leading_zeros() - 2;
    sa <<= la;
    ea -= la as i32;
    let exp = ep.max(ea);
    let sp = jam(sp, (exp - ep).min(200) as u32);
    let sa = jam(sa, (exp - ea).min(200) as u32);
    let (sign, sum) = if product_sign == c.sign {
        (c.sign, sp + sa)
    } else if sp >= sa {
        (product_sign, sp - sa)
    } else {
        (c.sign, sa - sp)
    };
    if sum == 0 {
        return f.zero(false);
    }
    round(f, sign, exp, sum, flags)
}

pub fn div(f: Format, a: u64, b: u64, flags: &mut u32) -> u64 {
    let x = unpack(f, a);
    let y = unpack(f, b);
    if let Some(nan) = process_nans(f, &[(a, x), (b, y)], flags) {
        return nan;
    }
    let sign = x.sign != y.sign;
    match (x.kind, y.kind) {
        (Kind::Infinity, Kind::Infinity) | (Kind::Zero, Kind::Zero) => {
            *flags |= IOC;
            f.default_nan()
        }
        (Kind::Infinity, _) => f.infinity(sign),
        (_, Kind::Infinity) | (Kind::Zero, _) => f.zero(sign),
        (_, Kind::Zero) => {
            *flags |= DZC;
            f.infinity(sign)
        }
        _ => {
            let (xs, xe) = normalise(x.sig, x.exp, f);
            let (ys, ye) = normalise(y.sig, y.exp, f);
            let numerator = (xs as u128) << 64;
            let mut q = numerator / ys as u128;
            if !numerator.is_multiple_of(ys as u128) {
                q |= 1;
            }
            round(f, sign, xe - ye - 64, q, flags)
        }
    }
}

fn normalise(sig: u64, exp: i32, f: Format) -> (u64, i32) {
    let shift = sig.leading_zeros() as i32 - (63 - f.frac_bits as i32);
    (sig << shift, exp - shift)
}

fn isqrt(n: u128) -> (u128, bool) {
    let mut root = 0u128;
    let mut rem = n;
    let mut bit = 1u128 << 126;
    while bit > n {
        bit >>= 2;
    }
    while bit != 0 {
        if rem >= root + bit {
            rem -= root + bit;
            root = (root >> 1) + bit;
        } else {
            root >>= 1;
        }
        bit >>= 2;
    }
    (root, rem != 0)
}

pub fn sqrt(f: Format, a: u64, flags: &mut u32) -> u64 {
    let x = unpack(f, a);
    if is_nan(&x) {
        return process_nan(f, a, &x, flags);
    }
    match x.kind {
        Kind::Zero => a & f.mask(),
        _ if x.sign => {
            *flags |= IOC;
            f.default_nan()
        }
        Kind::Infinity => a & f.mask(),
        _ => {
            let (mut sig, mut exp) = normalise(x.sig, x.exp, f);
            if exp & 1 != 0 {
                sig <<= 1;
                exp -= 1;
            }
            let (mut root, inexact) = isqrt((sig as u128) << 60);
            if inexact {
                root |= 1;
            }
            round(f, false, (exp - 60) / 2, root, flags)
        }
    }
}

/// FMAXNM (maximum = true) or FMINNM: one quiet NaN loses to a number.
pub fn min_max_num(f: Format, a: u64, b: u64, maximum: bool, flags: &mut u32) -> u64 {
    let mut a = a;
    let mut b = b;
    let x0 = unpack(f, a);
    let y0 = unpack(f, b);
    if x0.kind == Kind::Quiet && y0.kind != Kind::Quiet {
        a = f.infinity(maximum);
    } else if x0.kind != Kind::Quiet && y0.kind == Kind::Quiet {
        b = f.infinity(maximum);
    }
    let x = unpack(f, a);
    let y = unpack(f, b);
    if let Some(nan) = process_nans(f, &[(a, x), (b, y)], flags) {
        return nan;
    }
    if x.kind == Kind::Zero && y.kind == Kind::Zero {
        return f.zero(if maximum {
            x.sign && y.sign
        } else {
            x.sign || y.sign
        });
    }
    let first = compare(&x, &y);
    let pick_a = if maximum {
        first == std::cmp::Ordering::Greater
    } else {
        first == std::cmp::Ordering::Less
    };
    (if pick_a { a } else { b }) & f.mask()
}

fn magnitude_key(u: &Unpacked) -> (u8, i32, u64) {
    match u.kind {
        Kind::Zero => (0, 0, 0),
        Kind::Infinity => (2, 0, 0),
        _ => {
            let shift = u.sig.leading_zeros() as i32;
            (1, u.exp - shift, u.sig << shift)
        }
    }
}

fn compare(x: &Unpacked, y: &Unpacked) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    let zero = |u: &Unpacked| u.kind == Kind::Zero;
    if zero(x) && zero(y) {
        return Equal;
    }
    let sx = x.sign && !zero(x);
    let sy = y.sign && !zero(y);
    match (sx, sy) {
        (false, true) => Greater,
        (true, false) => Less,
        (false, false) => magnitude_key(x).cmp(&magnitude_key(y)),
        (true, true) => magnitude_key(y).cmp(&magnitude_key(x)),
    }
}

/// FPCompare/FPCompareEQ for FCMP (signal_all = false) and FCMPE: returns NZCV<<28.
pub fn fcmp(f: Format, a: u64, b: u64, signal_all: bool, flags: &mut u32) -> u64 {
    let x = unpack(f, a);
    let y = unpack(f, b);
    if is_nan(&x) || is_nan(&y) {
        if signal_all || x.kind == Kind::Signalling || y.kind == Kind::Signalling {
            *flags |= IOC;
        }
        return 0b0011 << 28;
    }
    match compare(&x, &y) {
        std::cmp::Ordering::Equal => 0b0110 << 28,
        std::cmp::Ordering::Less => 0b1000 << 28,
        std::cmp::Ordering::Greater => 0b0010 << 28,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rounding {
    /// FRINTM: toward minus infinity.
    Floor,
    /// FRINTP: toward plus infinity.
    Ceil,
    /// FRINTZ: toward zero.
    Trunc,
}

/// FRINTM/FRINTP/FRINTZ. These never raise Inexact (only FRINTX does).
pub fn round_int(f: Format, a: u64, mode: Rounding, flags: &mut u32) -> u64 {
    let x = unpack(f, a);
    if is_nan(&x) {
        return process_nan(f, a, &x, flags);
    }
    if x.kind != Kind::Finite || x.exp >= 0 {
        return a & f.mask();
    }
    let shift = (-x.exp) as u32;
    let (int, fraction) = if shift >= 64 {
        (0, x.sig != 0)
    } else {
        (x.sig >> shift, x.sig & ((1 << shift) - 1) != 0)
    };
    let away = fraction
        && match mode {
            Rounding::Floor => x.sign,
            Rounding::Ceil => !x.sign,
            Rounding::Trunc => false,
        };
    let int = int + u64::from(away);
    if int == 0 {
        return f.zero(x.sign);
    }
    round(f, x.sign, 0, int as u128, flags)
}

/// FCVTZS/FCVTZU: round toward zero, saturate, NaN -> 0; bits = 32 or 64.
pub fn to_int(f: Format, a: u64, signed: bool, bits: u32, flags: &mut u32) -> u64 {
    let x = unpack(f, a);
    let (min, max): (i128, i128) = if signed {
        (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
    } else {
        (0, (1i128 << bits) - 1)
    };
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let saturate = |negative: bool, flags: &mut u32| {
        *flags |= IOC;
        (if negative { min } else { max }) as u64 & mask
    };
    match x.kind {
        Kind::Quiet | Kind::Signalling => {
            *flags |= IOC;
            0
        }
        Kind::Zero => 0,
        Kind::Infinity => saturate(x.sign, flags),
        Kind::Finite => {
            if x.exp >= 64 {
                return saturate(x.sign, flags);
            }
            let (magnitude, inexact) = if x.exp >= 0 {
                ((x.sig as i128) << x.exp, false)
            } else {
                let shift = (-x.exp) as u32;
                if shift >= 64 {
                    (0, true)
                } else {
                    ((x.sig >> shift) as i128, x.sig & ((1 << shift) - 1) != 0)
                }
            };
            let value = if x.sign { -magnitude } else { magnitude };
            if value < min || value > max {
                return saturate(x.sign, flags);
            }
            if inexact {
                *flags |= IXC;
            }
            value as u64 & mask
        }
    }
}

/// SCVTF/UCVTF from a `bits`-wide integer.
pub fn from_int(f: Format, value: u64, signed: bool, bits: u32, flags: &mut u32) -> u64 {
    let value = if bits == 32 {
        value & 0xffff_ffff
    } else {
        value
    };
    let (sign, magnitude) = if signed {
        let v = if bits == 32 {
            value as u32 as i32 as i64
        } else {
            value as i64
        };
        (v < 0, v.unsigned_abs())
    } else {
        (false, value)
    };
    if magnitude == 0 {
        return f.zero(false);
    }
    round(f, sign, 0, magnitude as u128, flags)
}

/// FCVT between single and double precision.
pub fn convert(from: Format, to: Format, a: u64, flags: &mut u32) -> u64 {
    let x = unpack(from, a);
    let sign = x.sign;
    match x.kind {
        Kind::Quiet | Kind::Signalling => {
            if x.kind == Kind::Signalling {
                *flags |= IOC;
            }
            // FPConvertNaN: keep the sign and the most significant payload bits.
            let frac = a & from.frac_mask();
            let frac = if to.frac_bits >= from.frac_bits {
                frac << (to.frac_bits - from.frac_bits)
            } else {
                frac >> (from.frac_bits - to.frac_bits)
            };
            to.zero(sign) | (to.max_field() << to.frac_bits) | to.quiet_bit() | frac
        }
        Kind::Infinity => to.infinity(sign),
        Kind::Zero => to.zero(sign),
        Kind::Finite => round(to, sign, x.exp, x.sig as u128, flags),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(v: f64) -> u64 {
        v.to_bits()
    }
    #[test]
    fn ordinary_values_match_ieee_round_to_nearest() {
        let mut flags = 0;
        assert_eq!(add(F64, d(1.5), d(2.25), false, &mut flags), d(3.75));
        assert_eq!(flags, 0);
        assert_eq!(add(F64, d(0.1), d(0.2), false, &mut flags), d(0.1 + 0.2));
        assert_eq!(flags, IXC);
        assert_eq!(mul(F64, d(3.0), d(-0.5), &mut 0), d(-1.5));
        assert_eq!(div(F64, d(1.0), d(3.0), &mut 0), d(1.0 / 3.0));
        assert_eq!(sqrt(F64, d(2.0), &mut 0), d(2f64.sqrt()));
        assert_eq!(
            mul_add(F64, d(1.0), d(0.1), d(10.0), &mut 0),
            d(0.1f64.mul_add(10.0, 1.0))
        );
        assert_eq!(
            add(
                F32,
                1.5f32.to_bits() as u64,
                0.1f32.to_bits() as u64,
                false,
                &mut 0
            ),
            (1.5f32 + 0.1f32).to_bits() as u64
        );
    }
    #[test]
    fn arm_nan_and_zero_rules() {
        let snan = 0x7ff0_0000_0000_0002;
        let qnan = 0xfff8_0000_0000_0005;
        let mut flags = 0;
        assert_eq!(
            add(F64, qnan, snan, false, &mut flags),
            0x7ff8_0000_0000_0002
        );
        assert_eq!(flags, IOC);
        assert_eq!(add(F64, d(1.0), d(-1.0), false, &mut 0), 0);
        assert_eq!(add(F64, d(-0.0), d(-0.0), false, &mut 0), d(-0.0));
        let mut flags = 0;
        assert_eq!(
            mul_add(F64, qnan, d(0.0), d(f64::INFINITY), &mut flags),
            F64.default_nan()
        );
        assert_eq!(flags, IOC);
        let mut flags = 0;
        assert_eq!(mul(F64, 1, d(0.5), &mut flags), 0);
        assert_eq!(flags, UFC | IXC);
        let mut flags = 0;
        assert_eq!(mul(F64, d(f64::MAX), d(2.0), &mut flags), d(f64::INFINITY));
        assert_eq!(flags, OFC | IXC);
    }
}
