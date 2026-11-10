//! Restricted, checked loader. Guest addresses never become host pointers.
use crate::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub address: u64,
    pub bytes: std::sync::Arc<Vec<u8>>,
    pub protection: u32,
}

#[derive(Clone, Debug)]
pub struct Image {
    pub segments: Vec<Segment>,
    pub entry: u64,
    pub text: (u64, u64),
    pub extra_text: Vec<(u64, u64)>,
    pub platform: u32,
    pub startup: Option<Startup>,
}

#[derive(Clone, Debug)]
pub struct Startup {
    pub header: u64,
    pub initializers: Vec<u64>,
}

#[derive(Debug)]
pub(crate) struct InitSection {
    pub address: u64,
    pub size: usize,
    pub offsets: bool,
}

pub(crate) fn range(data: &[u8], offset: usize, size: usize) -> Result<&[u8]> {
    data.get(offset..offset.checked_add(size).ok_or("range overflow")?)
        .ok_or_else(|| "truncated Mach-O".into())
}
pub(crate) fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        range(data, offset, 4)?.try_into().unwrap(),
    ))
}
pub(crate) fn u16_at(data: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        range(data, offset, 2)?.try_into().unwrap(),
    ))
}
pub(crate) fn u64_at(data: &[u8], offset: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(
        range(data, offset, 8)?.try_into().unwrap(),
    ))
}
fn size(n: u64) -> Result<usize> {
    usize::try_from(n).map_err(|_| "size exceeds host address space".into())
}

#[derive(Debug)]
pub(crate) struct Object {
    pub image: Image,
    pub kind: u32,
    pub id: Option<String>,
    pub current_version: u32,
    pub dependencies: Vec<(String, u32)>,
    pub segment_indices: Vec<Option<usize>>,
    pub segment_file_sizes: Vec<u64>,
    pub readonly: Vec<usize>,
    pub streams: [Vec<u8>; 5],
    pub header_base: u64,
    pub chained: Option<Vec<u8>>,
    pub init_sections: Vec<InitSection>,
}

fn command_string(command: &[u8], offset: usize) -> Result<String> {
    if offset < 24 {
        return Err("invalid dylib name offset".into());
    }
    let tail = command.get(offset..).ok_or("dylib name out of bounds")?;
    let end = tail
        .iter()
        .position(|c| *c == 0)
        .ok_or("unterminated dylib name")?;
    if end == 0 || end > 4096 {
        return Err("invalid dylib name length".into());
    }
    String::from_utf8(tail[..end].to_vec()).map_err(|_| "non-UTF8 dylib name unsupported".into())
}

impl Object {
    pub fn parse(data: &[u8], dynamic: bool) -> Result<Self> {
        if u32_at(data, 0)? != 0xfeedfacf || u32_at(data, 4)? != 0x0100000c {
            return Err("expected thin little-endian ARM64 Mach-O".into());
        }
        if u32_at(data, 8)? & 0x00ffffff != 0 {
            return Err("arm64e/PAC is not supported".into());
        }
        let kind = u32_at(data, 12)?;
        if kind != 2 && !(dynamic && kind == 6) {
            return Err("expected MH_EXECUTE (or MH_DYLIB for explicit linking)".into());
        }
        let command_count = u32_at(data, 16)?;
        let command_end = 32usize
            .checked_add(u32_at(data, 20)? as usize)
            .ok_or("commands overflow")?;
        range(data, 32, command_end - 32)?;
        if command_count > 4096 {
            return Err("too many load commands".into());
        }
        let mut id = None;
        let mut current_version = 0;
        let mut dependencies = Vec::new();
        let mut segment_indices = Vec::new();
        let mut segment_file_sizes = Vec::new();
        let mut readonly = Vec::new();
        let mut header_base = None;
        let mut streams: [Vec<u8>; 5] = std::array::from_fn(|_| Vec::new());
        let mut dyld_info_seen = false;
        let mut chained = None;
        let mut exports_seen = false;
        let mut extra_text = Vec::new();
        let mut init_sections = Vec::new();
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
                    segment_file_sizes.push(file_size);
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
                        segment_indices.push(Some(segments.len()));
                        if file_offset == 0
                            && file_size > 0
                            && header_base.replace(address).is_some()
                        {
                            return Err("duplicate mapped Mach-O header".into());
                        }
                        if u32_at(c, 68)? & 0x10 != 0 {
                            readonly.push(segments.len());
                        }
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
                            bytes: bytes.into(),
                            protection,
                        });
                        entry_mappings.push((file_offset, file_size, address, protection));
                    } else {
                        segment_indices.push(None);
                        if file_size != 0 {
                            return Err("inaccessible segment contains file data".into());
                        }
                    }
                    for i in 0..sections {
                        let s = range(c, 72 + i * 80, 80)?;
                        if u32_at(s, 60)? != 0 {
                            return Err("section relocations unsupported".into());
                        }
                        let section_type = u32_at(s, 64)? & 0xff;
                        if (section_type == 9 || section_type == 0x16) && u64_at(s, 40)? != 0 {
                            if !dynamic {
                                return Err("initializers require explicit dynamic linking".into());
                            }
                            let start = u64_at(s, 32)?;
                            let length = u64_at(s, 40)?;
                            let alignment = if section_type == 0x16 { 4 } else { 8 };
                            if start % alignment != 0
                                || length % alignment != 0
                                || length > 8192
                                || start < address
                                || start
                                    .checked_add(length)
                                    .ok_or("initializer range overflow")?
                                    > address
                                        .checked_add(file_size)
                                        .ok_or("initializer file range overflow")?
                                || protection & 1 == 0
                                || (section_type == 0x16 && protection & 2 != 0)
                            {
                                return Err(
                                    "invalid initializer array bounds/alignment/protection".into(),
                                );
                            }
                            init_sections.push(InitSection {
                                address: start,
                                size: length as usize,
                                offsets: section_type == 0x16,
                            });
                        }
                        if (section_type == 10 || (0x11..=0x15).contains(&section_type))
                            && u64_at(s, 40)? != 0
                        {
                            return Err("initializers/terminators/TLS sections unsupported".into());
                        }
                        if &s[..16] == b"__text\0\0\0\0\0\0\0\0\0\0"
                            || u32_at(s, 64)? & 0x80000400 != 0
                        {
                            let start = u64_at(s, 32)?;
                            let len = u64_at(s, 40)?;
                            if kind == 6 && len == 0 {
                                continue;
                            }
                            if protection & 4 == 0
                                || start < address
                                || start.checked_add(len).ok_or("text overflow")? > end
                                || len == 0
                                || len % 4 != 0
                                || start % 4 != 0
                                || len > 1024 * 1024
                            {
                                return Err("invalid/duplicate executable __text".into());
                            }
                            if &s[..16] == b"__text\0\0\0\0\0\0\0\0\0\0" {
                                if text.replace((start, len)).is_some() {
                                    return Err("duplicate __text".into());
                                }
                            } else {
                                extra_text.push((start, len));
                            }
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
                    if dyld_info_seen {
                        return Err("duplicate dyld info".into());
                    }
                    dyld_info_seen = true;
                    let mut stream_bytes = 0usize;
                    for (index, offset) in [8, 16, 24, 32, 40].into_iter().enumerate() {
                        let at = u32_at(c, offset)? as usize;
                        let length = u32_at(c, offset + 4)? as usize;
                        stream_bytes = stream_bytes
                            .checked_add(length)
                            .ok_or("dyld stream size overflow")?;
                        if stream_bytes > 8 * 1024 * 1024 {
                            return Err("dyld streams exceed 8 MiB".into());
                        }
                        let payload = range(data, at, length)?;
                        if index == 4 && exports_seen && length != 0 {
                            return Err("duplicate export trie".into());
                        }
                        if index != 4 || length != 0 {
                            streams[index] = payload.to_vec();
                        }
                    }
                    for offset in [12, 20, 28, 36] {
                        if !dynamic && u32_at(c, offset)? != 0 {
                            return Err("dyld rebasing/binding unsupported".into());
                        }
                    }
                }
                0xb => {
                    range(c, 0, 80)?;
                    if (!dynamic && u32_at(c, 28)? != 0)
                        || u32_at(c, 68)? != 0
                        || u32_at(c, 76)? != 0
                    {
                        return Err("undefined symbols or dynamic relocations unsupported".into());
                    }
                }
                0x80000034 | 0x80000033 if dynamic => {
                    if length != 16 {
                        return Err("invalid chained fixups/exports command length".into());
                    }
                    let offset = u32_at(c, 8)? as usize;
                    let size = u32_at(c, 12)? as usize;
                    if size > 8 * 1024 * 1024 {
                        return Err("chained fixups/exports exceed 8 MiB".into());
                    }
                    let payload = range(data, offset, size)?.to_vec();
                    if cmd == 0x80000034 {
                        if chained.replace(payload).is_some() {
                            return Err("duplicate chained fixups".into());
                        }
                    } else {
                        if exports_seen || !streams[4].is_empty() {
                            return Err("duplicate export trie".into());
                        }
                        exports_seen = true;
                        streams[4] = payload;
                    }
                }
                0xc if dynamic => {
                    range(c, 0, 24)?;
                    if dependencies.len() >= 64 {
                        return Err("too many dylib dependencies".into());
                    }
                    dependencies.push((command_string(c, u32_at(c, 8)? as usize)?, u32_at(c, 20)?));
                }
                0xd if dynamic => {
                    range(c, 0, 24)?;
                    if kind != 6
                        || id
                            .replace(command_string(c, u32_at(c, 8)? as usize)?)
                            .is_some()
                    {
                        return Err("invalid/duplicate dylib identity".into());
                    }
                    current_version = u32_at(c, 16)?;
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
        if chained.is_some() && streams[..4].iter().any(|s| !s.is_empty()) {
            return Err("mixed legacy and chained fixups unsupported".into());
        }
        if kind == 6 && entry_offset.is_some() {
            return Err("dylib cannot declare LC_MAIN".into());
        }
        let offset = if kind == 2 {
            entry_offset.ok_or("LC_MAIN required (freestanding entry)")?
        } else {
            0
        };
        let entry = if kind == 6 {
            0
        } else {
            entry_mappings
                .iter()
                .find_map(|&(off, len, addr, prot)| {
                    (prot & 4 != 0 && offset >= off && offset - off < len)
                        .then(|| addr + (offset - off))
                })
                .ok_or("entry is outside executable file data")?
        };
        let text = text
            .or_else(|| (kind == 6).then_some((0, 0)))
            .ok_or("__text section required")?;
        if kind == 2 && (entry < text.0 || entry >= text.0 + text.1 || entry % 4 != 0) {
            return Err("entry outside __text".into());
        }
        let platform = platform.ok_or("LC_BUILD_VERSION required")?;
        if platform != 7 {
            return Err("expected PLATFORM_IOSSIMULATOR (7)".into());
        }
        let mut ranges = vec![text];
        ranges.extend(extra_text.iter().copied());
        ranges.sort_unstable();
        if ranges.windows(2).any(|r| r[0].0 + r[0].1 > r[1].0)
            || ranges.iter().map(|r| r.1).sum::<u64>() > 1024 * 1024
        {
            return Err("overlapping/excessive instruction ranges".into());
        }
        Ok(Self {
            image: Image {
                segments,
                entry: if kind == 6 { text.0 } else { entry },
                text,
                extra_text,
                platform,
                startup: None,
            },
            kind,
            id,
            current_version,
            dependencies,
            segment_indices,
            segment_file_sizes,
            readonly,
            streams,
            header_base: header_base.ok_or("missing mapped Mach-O header")?,
            chained,
            init_sections,
        })
    }
}

impl Image {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut object = Object::parse(data, false)?;
        for index in object.readonly {
            object.image.segments[index].protection &= !2;
        }
        Ok(object.image)
    }
    pub fn link(main: &[u8], libraries: &[&[u8]]) -> Result<Self> {
        crate::linker::link(main, libraries)
    }
    pub fn code_ranges(&self) -> impl Iterator<Item = (u64, u64)> + '_ {
        std::iter::once(self.text)
            .chain(self.extra_text.iter().copied())
            .filter(|(_, len)| *len != 0)
    }
    pub fn contains_pc(&self, pc: u64) -> bool {
        pc.is_multiple_of(4)
            && self
                .code_ranges()
                .any(|(start, len)| pc >= start && pc < start + len)
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
        if !self.contains_pc(pc) {
            return Err(format!("PC outside __text: {pc:#x}"));
        }
        Ok(u32::from_le_bytes(self.read(pc, 4, 4)?.try_into().unwrap()))
    }
}
