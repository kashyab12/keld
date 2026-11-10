//! Restricted, checked loader. Guest addresses never become host pointers.
use crate::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub address: u64,
    pub bytes: Vec<u8>,
    pub protection: u32,
}

#[derive(Clone, Debug)]
pub struct Image {
    pub segments: Vec<Segment>,
    pub entry: u64,
    pub text: (u64, u64),
    pub platform: u32,
}

fn range(data: &[u8], offset: usize, size: usize) -> Result<&[u8]> {
    data.get(offset..offset.checked_add(size).ok_or("range overflow")?)
        .ok_or_else(|| "truncated Mach-O".into())
}
fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        range(data, offset, 4)?.try_into().unwrap(),
    ))
}
fn u64_at(data: &[u8], offset: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(
        range(data, offset, 8)?.try_into().unwrap(),
    ))
}
fn size(n: u64) -> Result<usize> {
    usize::try_from(n).map_err(|_| "size exceeds host address space".into())
}

impl Image {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if u32_at(data, 0)? != 0xfeedfacf || u32_at(data, 4)? != 0x0100000c {
            return Err("expected thin little-endian ARM64 Mach-O".into());
        }
        if u32_at(data, 8)? & 0x00ffffff != 0 {
            return Err("arm64e/PAC is not supported".into());
        }
        if u32_at(data, 12)? != 2 {
            return Err("expected MH_EXECUTE".into());
        }
        let command_count = u32_at(data, 16)?;
        let command_end = 32usize
            .checked_add(u32_at(data, 20)? as usize)
            .ok_or("commands overflow")?;
        range(data, 32, command_end - 32)?;
        if command_count > 4096 {
            return Err("too many load commands".into());
        }
        let mut cursor = 32;
        let mut segments: Vec<Segment> = Vec::new();
        let mut entry_offset = None;
        let mut text = None;
        let mut platform = None;
        let mut total = 0u64;
        let mut entry_mappings = Vec::new();
        for _ in 0..command_count {
            if cursor + 8 > command_end {
                return Err("load command header exceeds sizeofcmds".into());
            }
            let cmd = u32_at(data, cursor)?;
            let length = u32_at(data, cursor + 4)? as usize;
            if length < 8
                || !length.is_multiple_of(8)
                || cursor.checked_add(length).ok_or("command overflow")? > command_end
            {
                return Err("invalid load command length".into());
            }
            let c = range(data, cursor, length)?;
            match cmd {
                0x19 => {
                    range(c, 0, 72)?;
                    let address = u64_at(c, 24)?;
                    let memory_size = u64_at(c, 32)?;
                    let file_offset = u64_at(c, 40)?;
                    let file_size = u64_at(c, 48)?;
                    let protection = u32_at(c, 60)?;
                    let sections = u32_at(c, 64)? as usize;
                    range(c, 72, sections.checked_mul(80).ok_or("sections overflow")?)?;
                    if memory_size < file_size || protection & !7 != 0 || protection & 6 == 6 {
                        return Err("invalid segment size/protection (W+X forbidden)".into());
                    }
                    let end = address
                        .checked_add(memory_size)
                        .ok_or("segment address overflow")?;
                    // __PAGEZERO reserves an inaccessible guest range, not physical storage.
                    if protection != 0 {
                        total = total.checked_add(memory_size).ok_or("memory overflow")?;
                        if total > 64 * 1024 * 1024 {
                            return Err("image exceeds 64 MiB research limit".into());
                        }
                        if segments
                            .iter()
                            .any(|s| address < s.address + s.bytes.len() as u64 && s.address < end)
                        {
                            return Err("overlapping guest segments".into());
                        }
                        let source = range(data, size(file_offset)?, size(file_size)?)?;
                        let mut bytes = vec![0; size(memory_size)?];
                        bytes[..source.len()].copy_from_slice(source);
                        segments.push(Segment {
                            address,
                            bytes,
                            protection,
                        });
                        entry_mappings.push((file_offset, file_size, address, protection));
                    } else if file_size != 0 {
                        return Err("inaccessible segment contains file data".into());
                    }
                    for i in 0..sections {
                        let s = range(c, 72 + i * 80, 80)?;
                        if u32_at(s, 60)? != 0 {
                            return Err("section relocations unsupported".into());
                        }
                        if &s[..16] == b"__text\0\0\0\0\0\0\0\0\0\0" {
                            let start = u64_at(s, 32)?;
                            let len = u64_at(s, 40)?;
                            if text.is_some()
                                || protection & 4 == 0
                                || start < address
                                || start.checked_add(len).ok_or("text overflow")? > end
                                || len == 0
                                || len % 4 != 0
                                || start % 4 != 0
                                || len > 1024 * 1024
                            {
                                return Err("invalid/duplicate executable __text".into());
                            }
                            text = Some((start, len));
                        }
                    }
                }
                0x80000028 => {
                    range(c, 0, 24)?;
                    if entry_offset.replace(u64_at(c, 8)?).is_some() {
                        return Err("duplicate LC_MAIN".into());
                    }
                }
                0x32 => {
                    range(c, 0, 24)?;
                    if platform.replace(u32_at(c, 8)?).is_some() {
                        return Err("duplicate LC_BUILD_VERSION".into());
                    }
                }
                0x22 | 0x80000022 => {
                    range(c, 0, 48)?;
                    for offset in [12, 20, 28, 36] {
                        if u32_at(c, offset)? != 0 {
                            return Err("dyld rebasing/binding unsupported".into());
                        }
                    }
                }
                0xb => {
                    range(c, 0, 80)?;
                    if u32_at(c, 28)? != 0 || u32_at(c, 68)? != 0 || u32_at(c, 76)? != 0 {
                        return Err("undefined symbols or dynamic relocations unsupported".into());
                    }
                }
                0x21 | 0x2c => return Err("encrypted images unsupported".into()),
                // These require a dynamic linker, bindings or relocations we do not implement.
                0xc | 0x18 | 0x80000018 | 0x8000001f | 0x80000023 | 0x80000034 | 0x80000033 => {
                    return Err(format!("unsupported dynamic load command {cmd:#x}"));
                }
                _ if cmd & 0x80000000 != 0 => {
                    return Err(format!("unknown required load command {cmd:#x}"));
                }
                // Harmless metadata we intentionally do not execute or load.
                0x2 | 0xe | 0x1b | 0x1d | 0x26 | 0x29 | 0x2a => {}
                _ => return Err(format!("unsupported load command {cmd:#x}")),
            }
            cursor += length;
        }
        if cursor != command_end {
            return Err("load commands disagree with sizeofcmds".into());
        }
        let offset = entry_offset.ok_or("LC_MAIN required (freestanding entry)")?;
        let entry = entry_mappings
            .iter()
            .find_map(|&(off, len, addr, prot)| {
                (prot & 4 != 0 && offset >= off && offset - off < len)
                    .then(|| addr + (offset - off))
            })
            .ok_or("entry is outside executable file data")?;
        let text = text.ok_or("__text section required")?;
        if entry < text.0 || entry >= text.0 + text.1 || entry % 4 != 0 {
            return Err("entry outside __text".into());
        }
        let platform = platform.ok_or("LC_BUILD_VERSION required")?;
        if platform != 7 {
            return Err("expected PLATFORM_IOSSIMULATOR (7)".into());
        }
        Ok(Self {
            segments,
            entry,
            text,
            platform,
        })
    }

    pub fn read(&self, address: u64, length: usize, permission: u32) -> Result<&[u8]> {
        for segment in &self.segments {
            if segment.protection & permission != permission {
                continue;
            }
            if let Some(offset) = address
                .checked_sub(segment.address)
                .and_then(|n| usize::try_from(n).ok())
                && let Some(end) = offset.checked_add(length)
                && let Some(bytes) = segment.bytes.get(offset..end)
            {
                return Ok(bytes);
            }
        }
        Err(format!(
            "guest memory fault at {address:#x}, length {length}, permission {permission}"
        ))
    }

    pub fn instruction(&self, pc: u64) -> Result<u32> {
        if !pc.is_multiple_of(4) || pc < self.text.0 || pc >= self.text.0 + self.text.1 {
            return Err(format!("PC outside __text: {pc:#x}"));
        }
        Ok(u32::from_le_bytes(self.read(pc, 4, 4)?.try_into().unwrap()))
    }
}
