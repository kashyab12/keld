//! Checked decoding of the unauthenticated 64-bit dyld chain formats.
//! Original implementation; binary layout reference: Apple's fixup-chains.h.
use crate::{
    Result,
    macho::{Object, range, u16_at, u32_at, u64_at},
};

const LIMIT: usize = 1_000_000;
type Patch = (usize, usize, u64);

/// Decode and resolve everything before changing any bytes. Encoded chain links
/// are read only from the original image, never from already rewritten pointers.
pub(crate) fn patches(
    object: &Object,
    slide: u64,
    mut resolve: impl FnMut(i64, &str) -> Result<crate::linker::Symbol>,
) -> Result<Vec<Patch>> {
    let Some(data) = &object.chained else {
        return Ok(Vec::new());
    };
    range(data, 0, 28)?;
    if u32_at(data, 0)? != 0 || u32_at(data, 24)? != 0 {
        return Err("unsupported chained fixups version/compressed symbols".into());
    }
    let starts = u32_at(data, 4)? as usize;
    let imports = u32_at(data, 8)? as usize;
    let symbols = u32_at(data, 12)? as usize;
    let count = u32_at(data, 16)? as usize;
    let format = u32_at(data, 20)?;
    let stride = match format {
        1 => 4usize,
        2 => 8,
        3 => 16,
        _ => return Err("unsupported chained import format".into()),
    };
    if count > LIMIT || starts < 28 || imports < starts + 4 {
        return Err("invalid/excessive chained header offsets/count".into());
    }
    let import_bytes = count
        .checked_mul(stride)
        .ok_or("chained import size overflow")?;
    range(data, imports, import_bytes)?;
    if symbols < imports + import_bytes || symbols > data.len() {
        return Err("chained imports/symbols overlap or exceed payload".into());
    }
    let seg_count = u32_at(data, starts)? as usize;
    if seg_count != object.segment_indices.len() {
        return Err("chained segment count does not match image".into());
    }
    let table_size = 4 + seg_count * 4;
    range(data, starts, table_size)?;
    if starts + table_size > imports {
        return Err("chained starts/imports overlap".into());
    }
    let mut bindings = Vec::with_capacity(count);
    for i in 0..count {
        let at = imports + i * stride;
        let (ordinal, name, addend) = if format == 3 {
            let raw = u64_at(data, at)?;
            if (raw >> 16) & 0xffff != 0 {
                return Err("weak/reserved chained import unsupported".into());
            }
            let ordinal = (raw & 0xffff) as i64;
            (
                if ordinal > 0xfff0 {
                    ordinal - 0x10000
                } else {
                    ordinal
                },
                (raw >> 32) as usize,
                u64_at(data, at + 8)? as i64,
            )
        } else {
            let raw = u32_at(data, at)?;
            if raw & 0x100 != 0 {
                return Err("weak chained import unsupported".into());
            }
            let ordinal = (raw & 0xff) as i64;
            (
                if ordinal > 0xf0 {
                    ordinal - 0x100
                } else {
                    ordinal
                },
                (raw >> 9) as usize,
                if format == 2 {
                    u32_at(data, at + 4)? as i32 as i64
                } else {
                    0
                },
            )
        };
        let name = symbols
            .checked_add(name)
            .ok_or("chained symbol offset overflow")?;
        let tail = data.get(name..).ok_or("chained symbol out of bounds")?;
        let length = tail
            .iter()
            .take(4097)
            .position(|b| *b == 0)
            .ok_or("unterminated/excessive chained symbol")?;
        if length == 0 {
            return Err("empty chained symbol".into());
        }
        let name = std::str::from_utf8(&tail[..length]).map_err(|_| "non-UTF8 chained symbol")?;
        let value = resolve(ordinal, name)?.addend(addend)?;
        bindings.push(value);
    }
    let base = object
        .header_base
        .checked_add(slide)
        .ok_or("chained base overflow")?;
    let mut patches = Vec::new();
    let mut info_ranges = Vec::new();
    for seg in 0..seg_count {
        let relative = u32_at(data, starts + 4 + seg * 4)? as usize;
        if relative == 0 {
            continue;
        }
        if relative < table_size {
            return Err("chained segment info overlaps starts table".into());
        }
        let at = starts
            .checked_add(relative)
            .ok_or("chained info offset overflow")?;
        let size = u32_at(data, at)? as usize;
        let end = at.checked_add(size).ok_or("chained info size overflow")?;
        if size < 22 || end > imports || info_ranges.iter().any(|&(a, b)| at < b && a < end) {
            return Err("invalid/overlapping chained segment info".into());
        }
        info_ranges.push((at, end));
        let info = range(data, at, size)?;
        let page_size = u16_at(info, 4)? as usize;
        let pointer_format = u16_at(info, 6)?;
        let page_count = u16_at(info, 20)? as usize;
        if ![4096, 16384].contains(&page_size)
            || ![2, 6].contains(&pointer_format)
            || u32_at(info, 16)? != 0
        {
            return Err("unsupported chained page/pointer format (ARM64e/PAC excluded)".into());
        }
        range(info, 22, page_count * 2)?;
        let index = object.segment_indices[seg].ok_or("chained fixup in inaccessible segment")?;
        let segment = &object.image.segments[index];
        if segment.protection & 4 != 0
            || segment.address.checked_sub(base) != Some(u64_at(info, 8)?)
        {
            return Err("chained segment offset/protection mismatch".into());
        }
        if page_count > segment.bytes.len().div_ceil(page_size) {
            return Err("chained page count exceeds segment".into());
        }
        for page in 0..page_count {
            let start = u16_at(info, 22 + page * 2)?;
            if start == 0xffff {
                continue;
            }
            if start & 0x8000 != 0 {
                return Err("multiple chained starts per page unsupported".into());
            }
            let page_offset = page * page_size;
            let mut offset = start as usize;
            loop {
                if !offset.is_multiple_of(4) || offset + 8 > page_size {
                    return Err("chained pointer crosses page or is misaligned".into());
                }
                let slot = page_offset + offset;
                if slot + 8 > object.segment_file_sizes[seg] as usize {
                    return Err("chained pointer outside file-backed segment".into());
                }
                let raw = u64_at(&segment.bytes, slot)?;
                let next = ((raw >> 51) & 0xfff) as usize * 4;
                let value = if raw >> 63 != 0 {
                    if (raw >> 32) & 0x7ffff != 0 {
                        return Err("nonzero chained bind reserved bits".into());
                    }
                    let binding = *bindings
                        .get((raw & 0xffffff) as usize)
                        .ok_or("chained bind ordinal out of bounds")?;
                    binding.addend(((raw >> 24) & 0xff) as i64)?.address
                } else {
                    if (raw >> 44) & 0x7f != 0 {
                        return Err("nonzero chained rebase reserved bits".into());
                    }
                    let target = raw & 0xfffffffff;
                    let high = ((raw >> 36) & 0xff) << 56;
                    if pointer_format == 2 {
                        target.checked_add(slide).ok_or("chained rebase overflow")? | high
                    } else {
                        (target | high)
                            .checked_add(base)
                            .ok_or("chained offset rebase overflow")?
                    }
                };
                if patches.len() == LIMIT {
                    return Err("excessive chained fixup count".into());
                }
                patches.push((index, slot, value));
                if next == 0 {
                    break;
                }
                if next < 8 {
                    return Err("overlapping chained pointer slots".into());
                }
                offset = offset.checked_add(next).ok_or("chained stride overflow")?;
            }
        }
    }
    Ok(patches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macho::Segment;

    fn put16(data: &mut [u8], at: usize, value: u16) {
        data[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }
    fn put32(data: &mut [u8], at: usize, value: u32) {
        data[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn put64(data: &mut [u8], at: usize, value: u64) {
        data[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn object(pointer_format: u16, import_format: u32, slide: u64) -> Object {
        let mut object =
            Object::parse(include_bytes!("../fixtures/bin/hello.macho"), true).unwrap();
        object.header_base = 0x1_0000_0000;
        object.segment_indices = vec![Some(0)];
        object.segment_file_sizes = vec![4096];
        let mut bytes = vec![0; 4096];
        // A bound pointer followed eight bytes later by a rebased pointer.
        put64(&mut bytes, 16, (1 << 63) | (2 << 51) | (9 << 24));
        put64(
            &mut bytes,
            24,
            if pointer_format == 2 {
                0x1_0000_0080 | (0xab << 36)
            } else {
                0x80 | (0xab << 36)
            },
        );
        object.image.segments = vec![Segment {
            address: object.header_base + slide + 4096,
            bytes: bytes.into(),
            protection: 3,
        }];
        let stride = match import_format {
            1 => 4,
            2 => 8,
            3 => 16,
            _ => unreachable!(),
        };
        let mut data = vec![0; 64 + stride + 8];
        for (at, value) in [
            (4, 28),
            (8, 64),
            (12, (64 + stride) as u32),
            (16, 1),
            (20, import_format),
            (28, 1),
            (32, 8),
        ] {
            put32(&mut data, at, value);
        }
        put32(&mut data, 36, 24);
        put16(&mut data, 40, 4096);
        put16(&mut data, 42, pointer_format);
        put64(&mut data, 44, 4096);
        put16(&mut data, 56, 1);
        put16(&mut data, 58, 16);
        if import_format == 3 {
            put64(&mut data, 64, 1);
            put64(&mut data, 72, (-0x1_0000_0001i64) as u64);
        } else {
            put32(&mut data, 64, 1);
            if import_format == 2 {
                put32(&mut data, 68, (-8i32) as u32);
            }
        }
        data[64 + stride..].copy_from_slice(b"_target\0");
        object.chained = Some(data);
        object
    }

    #[test]
    fn tls_import_and_inline_addends_reject_before_patching() {
        for pointer_format in [2, 6] {
            for import_format in [1, 2, 3] {
                let object = object(pointer_format, import_format, 0);
                let before = object.image.segments.clone();
                let error = patches(&object, 0, |_, _| {
                    Ok(crate::linker::Symbol {
                        address: 0x10_0000_0000,
                        tls: true,
                    })
                })
                .unwrap_err();
                assert!(error.contains("TLS descriptor addends"));
                assert_eq!(before, object.image.segments);
            }
        }
    }
    #[test]
    fn both_pointer_formats_import_addends_tags_and_slide() {
        for pointer_format in [2, 6] {
            for import_format in [1, 2, 3] {
                for slide in [0, 0x2000] {
                    let object = object(pointer_format, import_format, slide);
                    let got = patches(&object, slide, |ordinal, name| {
                        assert_eq!((ordinal, name), (1, "_target"));
                        Ok(crate::linker::Symbol {
                            address: 0x10_0000_0000,
                            tls: false,
                        })
                    })
                    .unwrap();
                    let addend = match import_format {
                        1 => 0,
                        2 => -8,
                        3 => -0x1_0000_0001,
                        _ => unreachable!(),
                    };
                    assert_eq!(
                        got,
                        vec![
                            (
                                0,
                                16,
                                0x10_0000_0000u64.checked_add_signed(addend).unwrap() + 9
                            ),
                            (0, 24, 0xab00_0001_0000_0080 + slide)
                        ]
                    );
                }
            }
        }
    }

    #[test]
    fn rejects_malformed_metadata_and_pointer_slots_without_mutating() {
        let changes: &[(usize, u32)] = &[
            (0, 1),
            (4, 0),
            (8, 0),
            (12, 64),
            (16, 1_000_001),
            (20, 99),
            (24, 1),
            (28, 2),
            (32, 4),
            (36, 0xffff_ffff),
            (52, 1),
        ];
        for &(at, value) in changes {
            let mut object = object(2, 1, 0);
            put32(object.chained.as_mut().unwrap(), at, value);
            let before = object.image.segments.clone();
            assert!(
                patches(&object, 0, |_, _| Ok(crate::linker::Symbol {
                    address: 100,
                    tls: false
                }))
                .is_err(),
                "offset {at}"
            );
            assert_eq!(object.image.segments, before);
        }
        for (at, value) in [
            (40, 8192),
            (42, 1),
            (56, 2),
            (58, 4092),
            (58, 0x8000),
            (58, 17),
        ] {
            let mut object = object(2, 1, 0);
            put16(object.chained.as_mut().unwrap(), at, value);
            assert!(
                patches(&object, 0, |_, _| Ok(crate::linker::Symbol {
                    address: 100,
                    tls: false
                }))
                .is_err()
            );
        }
        for raw in [
            (1 << 63) | 1,
            (1 << 63) | (1 << 32),
            (1 << 63) | (1 << 51),
            (1 << 44),
        ] {
            let mut object = object(2, 1, 0);
            put64(
                std::sync::Arc::make_mut(&mut object.image.segments[0].bytes).as_mut_slice(),
                16,
                raw,
            );
            assert!(
                patches(&object, 0, |_, _| Ok(crate::linker::Symbol {
                    address: 100,
                    tls: false
                }))
                .is_err()
            );
        }
        let mut object = object(2, 1, 0);
        object.segment_file_sizes[0] = 20;
        assert!(
            patches(&object, 0, |_, _| Ok(crate::linker::Symbol {
                address: 100,
                tls: false
            }))
            .is_err()
        );
        object.segment_file_sizes[0] = 4096;
        object.image.segments[0].protection = 5;
        assert!(
            patches(&object, 0, |_, _| Ok(crate::linker::Symbol {
                address: 100,
                tls: false
            }))
            .is_err()
        );
    }

    #[test]
    fn malformed_chains_are_bounded_and_never_panic() {
        let mut random = 0x5ab1eu64;
        for _ in 0..2000 {
            let mut object = object(2, 1, 0);
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            let data = object.chained.as_mut().unwrap();
            let at = (random >> 32) as usize % data.len();
            data[at] ^= (random as u8) | 1;
            let _ = patches(&object, 0, |_, _| {
                Ok(crate::linker::Symbol {
                    address: 100,
                    tls: false,
                })
            });
        }
    }
}
