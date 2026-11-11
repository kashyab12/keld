//! Typed Objective-C method type-encoding walker.
//!
//! Accepts the scalar types `B c i q Q f d @ # :` and the exact clang encodings of
//! CGPoint, CGSize, CGRect and _NSRange, and checks every frame offset the way
//! clang computes it: arguments start at 16 after self/_cmd, each advances by its
//! size rounded up to 4 bytes, and the leading frame size equals the final offset.
//! Everything must also fit the AAPCS64 argument registers (x0-x7, v0-v7) since
//! Sable never marshals stack-passed arguments. Blocks, unions, bitfields,
//! pointers, arrays, qualifiers, other structs and HFAs over 4 members reject.
use crate::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgType {
    Bool,
    Char,
    Int,
    Long,
    ULong,
    Float,
    Double,
    Object,
    Class,
    Selector,
    Point,
    Size,
    Rect,
    Range,
}

/// Exact struct encodings clang emits (checked against Xcode clang output).
const STRUCTS: [(&str, ArgType); 4] = [
    ("{CGPoint=dd}", ArgType::Point),
    ("{CGSize=dd}", ArgType::Size),
    ("{CGRect={CGPoint=dd}{CGSize=dd}}", ArgType::Rect),
    ("{_NSRange=QQ}", ArgType::Range),
];

impl ArgType {
    pub fn size(self) -> usize {
        match self {
            Self::Bool | Self::Char => 1,
            Self::Int | Self::Float => 4,
            Self::Long
            | Self::ULong
            | Self::Double
            | Self::Object
            | Self::Class
            | Self::Selector => 8,
            Self::Point | Self::Size | Self::Range => 16,
            Self::Rect => 32,
        }
    }
    /// (general registers, FP/SIMD registers) used when passed in registers.
    pub fn registers(self) -> (usize, usize) {
        match self {
            Self::Float | Self::Double => (0, 1),
            Self::Point | Self::Size => (0, 2),
            Self::Rect => (0, 4),
            Self::Range => (2, 0),
            _ => (1, 0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    /// `None` for a void return.
    pub ret: Option<ArgType>,
    pub args: Vec<ArgType>,
    pub frame: usize,
}

fn parse_type(s: &str) -> Result<(ArgType, &str)> {
    let first = s
        .as_bytes()
        .first()
        .copied()
        .ok_or("truncated ObjC type encoding")?;
    let simple = match first {
        b'B' => Some(ArgType::Bool),
        b'c' => Some(ArgType::Char),
        b'i' => Some(ArgType::Int),
        b'q' => Some(ArgType::Long),
        b'Q' => Some(ArgType::ULong),
        b'f' => Some(ArgType::Float),
        b'd' => Some(ArgType::Double),
        b'#' => Some(ArgType::Class),
        b':' => Some(ArgType::Selector),
        b'@' => match s.as_bytes().get(1) {
            Some(b'?') => return Err("unsupported ObjC block type encoding".into()),
            Some(b'"') => return Err("unsupported ObjC named object type encoding".into()),
            _ => Some(ArgType::Object),
        },
        _ => None,
    };
    if let Some(ty) = simple {
        return Ok((ty, &s[1..]));
    }
    match first {
        b'{' => {
            for (encoding, ty) in STRUCTS {
                if let Some(rest) = s.strip_prefix(encoding) {
                    return Ok((ty, rest));
                }
            }
            let end = struct_end(s)?;
            let body = &s[..end];
            let members = body.split_once('=').map_or("", |(_, m)| &m[..m.len() - 1]);
            if body.contains('(') {
                Err("unsupported ObjC union type encoding".into())
            } else if members.contains('b') {
                Err("unsupported ObjC bitfield type encoding".into())
            } else if !members.is_empty()
                && (members.bytes().all(|b| b == b'd') || members.bytes().all(|b| b == b'f'))
                && members.len() > 4
            {
                Err("unsupported ObjC HFA with more than 4 members".into())
            } else {
                Err(format!("unsupported ObjC struct type encoding {body}"))
            }
        }
        b'(' => Err("unsupported ObjC union type encoding".into()),
        b'b' => Err("unsupported ObjC bitfield type encoding".into()),
        b'v' => Err("void is only a return type".into()),
        _ => Err(format!(
            "unsupported ObjC type encoding {:?}",
            char::from(first)
        )),
    }
}

fn struct_end(s: &str) -> Result<usize> {
    let mut depth = 0usize;
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'{' | b'(' => depth += 1,
            b'}' | b')' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or("unbalanced ObjC struct encoding")?;
                if depth == 0 {
                    return Ok(i + 1);
                }
            }
            _ => {}
        }
    }
    Err("unbalanced ObjC struct encoding".into())
}

fn parse_offset(s: &str) -> Result<(usize, &str)> {
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    let text = &s[..digits];
    if digits == 0 || (digits > 1 && text.starts_with('0')) {
        return Err("unsupported ObjC frame offset".into());
    }
    let value = text
        .parse::<usize>()
        .ok()
        .filter(|v| *v <= 4096)
        .ok_or("unsupported ObjC frame offset")?;
    Ok((value, &s[digits..]))
}

/// Walk a method type encoding. `ordinary_method` = method lists; protocol
/// descriptions keep the existing unsigned-only (`Q`) profile.
pub fn method_signature(name: &str, encoding: &str, ordinary_method: bool) -> Result<Signature> {
    if matches!(name, "load" | "initialize") {
        return Err("unsupported/duplicate ObjC scalar method".into());
    }
    let count = name.bytes().filter(|&b| b == b':').count();
    let (ret, rest) = if let Some(rest) = encoding.strip_prefix('v') {
        if !ordinary_method {
            return Err("unsupported ObjC protocol void return".into());
        }
        (None, rest)
    } else {
        let (ty, rest) = parse_type(encoding)?;
        (Some(ty), rest)
    };
    let (frame, rest) = parse_offset(rest)?;
    let mut rest = rest
        .strip_prefix("@0:8")
        .ok_or("unsupported ObjC scalar frame encoding")?;
    let mut offset = 16;
    let mut args = Vec::new();
    let (mut general, mut vector) = (2, 0);
    let profile = |ty: ArgType| ordinary_method || ty == ArgType::ULong;
    if let Some(ty) = ret
        && !profile(ty)
    {
        return Err("unsupported ObjC protocol scalar type".into());
    }
    while !rest.is_empty() {
        let (ty, after) = parse_type(rest)?;
        if !profile(ty) {
            return Err("unsupported ObjC protocol scalar type".into());
        }
        let (at, after) = parse_offset(after)?;
        if at != offset {
            return Err("unsupported ObjC scalar argument offset".into());
        }
        let (g, v) = ty.registers();
        general += g;
        vector += v;
        if general > 8 || vector > 8 {
            return Err("unsupported ObjC stack-passed argument".into());
        }
        offset += ty.size().max(4);
        args.push(ty);
        rest = after;
    }
    if args.len() != count {
        return Err("ObjC selector/encoding argument count mismatch".into());
    }
    if frame != offset {
        return Err("unsupported ObjC scalar frame size".into());
    }
    Ok(Signature { ret, args, frame })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_clang_encodings_with_real_offsets() {
        for (name, ty) in [
            ("setFrame:", "v48@0:8{CGRect={CGPoint=dd}{CGSize=dd}}16"),
            (
                "initWithFrame:",
                "@48@0:8{CGRect={CGPoint=dd}{CGSize=dd}}16",
            ),
            ("bounds", "{CGRect={CGPoint=dd}{CGSize=dd}}16@0:8"),
            (
                "view:inRect:",
                "v56@0:8@16{CGRect={CGPoint=dd}{CGSize=dd}}24",
            ),
            (
                "rect:in:b:",
                "v60@0:8@16{CGRect={CGPoint=dd}{CGSize=dd}}24B56",
            ),
            ("p:s:", "v48@0:8{CGPoint=dd}16{CGSize=dd}32"),
            ("range:", "{_NSRange=QQ}32@0:8{_NSRange=QQ}16"),
            ("b:c:i:q:Q:f:", "v48@0:8B16c20i24q28Q36f44"),
            ("d:o:k:s:", "d48@0:8d16@24#32:40"),
            ("colorWithRed:green:blue:alpha:", "@48@0:8d16d24d32d40"),
            ("bf:f:", "v24@0:8f16f20"),
            ("respondsToSelector:", "B24@0:8:16"),
            ("class", "#16@0:8"),
            (
                "application:didFinishLaunchingWithOptions:",
                "B32@0:8@16@24",
            ),
        ] {
            let sig = method_signature(name, ty, true).unwrap_or_else(|e| panic!("{ty}: {e}"));
            assert_eq!(sig.args.len(), name.matches(':').count());
        }
        let sig = method_signature(
            "view:inRect:",
            "v56@0:8@16{CGRect={CGPoint=dd}{CGSize=dd}}24",
            true,
        )
        .unwrap();
        assert_eq!(sig.args, [ArgType::Object, ArgType::Rect]);
        assert_eq!(sig.frame, 56);
    }
    #[test]
    fn rejects_excluded_shapes_and_wrong_offsets() {
        for (name, ty, why) in [
            ("run:", "v24@0:8@?16", "block"),
            ("u:", "v24@0:8(U=id)16", "union"),
            ("s:", "v24@0:8{S=b1b7}16", "bitfield"),
            ("h:", "v56@0:8{H=ddddd}16", "more than 4 members"),
            ("h:", "v36@0:8{H=fffff}16", "more than 4 members"),
            ("r:", "v48@0:8{CGRect=dddd}16", "struct"),
            ("p:", "v24@0:8^v16", "type encoding"),
            ("a:", "v24@0:8[4d]16", "type encoding"),
            ("o:", "v24@0:8@\"UIView\"16", "named object"),
            ("c:", "v24@0:8r*16", "type encoding"),
            ("v:", "v24@0:8v16", "void"),
            ("s:", "v20@0:8s16", "type encoding"),
            ("i:", "v24@0:8i16", "frame size"),
            ("q:", "v24@0:8q20", "offset"),
            ("q:", "v024@0:8q16", "offset"),
            ("q:", "v24@0:8q016", "offset"),
            ("q:q:", "v24@0:8q16", "count"),
            ("x", "v16@0:8q16", "count"),
            (
                "r:r:r:",
                "v112@0:8{CGRect={CGPoint=dd}{CGSize=dd}}16{CGRect={CGPoint=dd}{CGSize=dd}}48{CGRect={CGPoint=dd}{CGSize=dd}}80",
                "stack-passed",
            ),
            (
                "a:b:c:d:e:f:g:",
                "v72@0:8q16q24q32q40q48q56q64",
                "stack-passed",
            ),
            ("load", "v16@0:8", "unsupported"),
        ] {
            let err = method_signature(name, ty, true).unwrap_err();
            assert!(err.contains(why), "{ty}: {err}");
        }
        // Protocol descriptions keep the unsigned-only profile.
        assert!(method_signature("value:", "Q24@0:8Q16", false).is_ok());
        assert!(method_signature("value:", "Q24@0:8d16", false).is_err());
        assert!(method_signature("value", "v16@0:8", false).is_err());
    }
}
