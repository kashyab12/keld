//! Original bounded ObjC2 registration and uncached scalar dispatch.
//! No Apple runtime code, host objects, allocation, ARC or Foundation services.
use crate::{
    Result,
    macho::{Image, Object, u32_at, u64_at},
    memory::Memory,
};
pub(crate) mod protocol;
use std::collections::{BTreeMap, BTreeSet};

pub const GET_CLASS: u32 = 0xd400_0001 | (0x5ab2 << 5);
pub const LOOKUP: u32 = 0xd400_0001 | (0x5ab3 << 5);
const RET: u32 = 0xd65f_03c0;
const BR_X16: u32 = 0xd61f_0200;
const CAP: usize = 256 * 1024;

#[derive(Debug)]
struct Section {
    name: String,
    address: u64,
    length: usize,
}
#[derive(Debug, Default)]
pub(crate) struct Sections {
    parts: Vec<Section>,
    ordinary_data: Vec<protocol::DataSection>,
    data_overflow: bool,
}
fn metadata_not_code(
    address: u64,
    length: usize,
    ranges: impl Iterator<Item = (u64, u64)>,
) -> Result<()> {
    let end = address
        .checked_add(length as u64)
        .ok_or("ObjC metadata range overflow")?;
    for (code, size) in ranges {
        let code_end = code
            .checked_add(size)
            .ok_or("ObjC instruction range overflow")?;
        if length != 0 && size != 0 && address < code_end && code < end {
            return Err("ObjC metadata overlaps guest instruction range".into());
        }
    }
    Ok(())
}
impl Sections {
    pub(crate) fn validate_code_ranges(&self, ranges: &[(u64, u64)]) -> Result<()> {
        for part in &self.parts {
            metadata_not_code(part.address, part.length, ranges.iter().copied())?;
        }
        Ok(())
    }
    pub fn push(
        &mut self,
        s: &[u8],
        segment: (u64, u64, u64, u64, u32),
        dynamic: bool,
    ) -> Result<()> {
        let name = std::str::from_utf8(&s[..16])
            .map_err(|_| "invalid ObjC section name")?
            .trim_end_matches('\0');
        if name == "__data" {
            if dynamic {
                if self.ordinary_data.len() == 64 {
                    self.data_overflow = true;
                } else {
                    self.ordinary_data
                        .push(protocol::DataSection::new(s, segment));
                }
            }
            return Ok(());
        }
        if (!name.starts_with("__objc_") && name != "__cstring") || u64_at(s, 40)? == 0 {
            return Ok(());
        }
        if name == "__objc_stubs" {
            if segment.4 != 5 || u32_at(s, 64)? & 0x80000400 == 0 {
                return Err("invalid ObjC selector stubs".into());
            }
            return Ok(()); // The Mach-O loader validates these immutable code ranges.
        }
        if !dynamic && name == "__cstring" {
            return Ok(());
        }
        if !dynamic {
            return Err("ObjC requires explicit dynamic linking".into());
        }
        if !matches!(
            name,
            "__objc_imageinfo"
                | "__objc_classlist"
                | "__objc_classrefs"
                | "__objc_selrefs"
                | "__objc_protolist"
                | "__objc_protorefs"
                | "__objc_const"
                | "__objc_methlist"
                | "__objc_data"
                | "__objc_methname"
                | "__objc_methtype"
                | "__objc_classname"
                | "__objc_ivar"
                | "__cstring"
        ) {
            return Err(format!("unsupported ObjC section: {name}"));
        }
        let (base, _, file_size, file_offset, protection) = segment;
        let address = u64_at(s, 32)?;
        let length = u64_at(s, 40)?;
        let exponent = u32_at(s, 52)?;
        let end = address.checked_add(length).ok_or("ObjC section overflow")?;
        let is_string = matches!(
            name,
            "__objc_methname" | "__objc_methtype" | "__objc_classname" | "__cstring"
        );
        let pointer_array = matches!(
            name,
            "__objc_classlist"
                | "__objc_classrefs"
                | "__objc_selrefs"
                | "__objc_protolist"
                | "__objc_protorefs"
        );
        if exponent > 12
            || (name != "__cstring" && length > CAP as u64)
            || (protection & 1 == 0
                || (!is_string && name != "__objc_methlist" && protection & 4 != 0))
            || address < base
            || end > base.checked_add(file_size).ok_or("ObjC segment overflow")?
            || address % (1u64 << exponent) != 0
            || u64::from(u32_at(s, 48)?)
                != file_offset
                    .checked_add(address - base)
                    .ok_or("ObjC file offset overflow")?
            || !(u32_at(s, 64)? & 255
                == if is_string {
                    2
                } else if name == "__objc_selrefs" {
                    5
                } else if name == "__objc_protorefs" {
                    11
                } else {
                    0
                }
                || (name == "__objc_protolist" && u32_at(s, 64)? & 255 == 11))
            || (pointer_array
                && (!address.is_multiple_of(8) || length % 8 != 0 || length / 8 > 1024))
            || (name == "__objc_methlist"
                && (protection != 5
                    || u32_at(s, 64)? & 0x80000400 != 0
                    || !address.is_multiple_of(4)
                    || length % 4 != 0))
            || (name == "__objc_imageinfo" && (length != 8 || address % 4 != 0))
        {
            return Err("invalid ObjC section bounds/alignment/type/protection".into());
        }
        if self
            .parts
            .iter()
            .any(|p| p.name == name || (address < p.address + p.length as u64 && p.address < end))
        {
            return Err("duplicate/overlapping ObjC section".into());
        }
        if name != "__cstring"
            && self
                .parts
                .iter()
                .filter(|p| p.name != "__cstring")
                .map(|p| p.length)
                .sum::<usize>()
                + length as usize
                > CAP
        {
            return Err("ObjC metadata exceeds cap".into());
        }
        self.parts.push(Section {
            name: name.into(),
            address,
            length: length as usize,
        });
        Ok(())
    }
    pub fn is_empty(&self) -> bool {
        self.parts.iter().all(|p| p.name == "__cstring")
    }
    fn section(&self, name: &str, slide: u64) -> Result<Option<(u64, usize)>> {
        self.parts
            .iter()
            .find(|p| p.name == name)
            .map(|p| {
                Ok((
                    p.address.checked_add(slide).ok_or("ObjC slide overflow")?,
                    p.length,
                ))
            })
            .transpose()
    }
    fn contains(&self, name: &str, slide: u64, address: u64, size: usize) -> Result<()> {
        if let Some((base, len)) = self.section(name, slide)?
            && address
                .checked_sub(base)
                .and_then(|o| o.checked_add(size as u64))
                .is_some_and(|end| end <= len as u64)
        {
            return Ok(());
        }
        Err(format!("ObjC record outside owning {name}"))
    }
}

#[derive(Clone, Debug)]
struct Method {
    selector: u64,
    imp: u64,
}
#[derive(Clone, Debug)]
struct Class {
    address: u64,
    meta: u64,
    name: String,
    instance: Vec<Method>,
    class: Vec<Method>,
    protocols: Vec<u64>,
}
#[derive(Clone, Debug)]
pub struct Plan {
    classes: Vec<Class>,
    selectors: BTreeSet<u64>,
    snapshots: Vec<(u64, Vec<u8>)>,
    runtime: Runtime,
    protocols: protocol::Plan,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Runtime {
    pub get_class: u64,
    pub msg_send: u64,
    pub empty_cache: u64,
    pub protocols: Option<protocol::Runtime>,
}

fn string(read: impl Fn(u64, usize) -> Result<Vec<u8>>, address: u64) -> Result<String> {
    let mut out = Vec::new();
    for at in 0..256u64 {
        let b = read(address.checked_add(at).ok_or("ObjC string overflow")?, 1)?[0];
        if b == 0 {
            if out.is_empty() {
                return Err("empty ObjC string".into());
            }
            return String::from_utf8(out).map_err(|_| "invalid ObjC string".into());
        }
        if !(32..127).contains(&b) {
            return Err("non-ASCII ObjC string unsupported".into());
        }
        out.push(b);
    }
    Err("unterminated/excessive ObjC string".into())
}
fn image_string(image: &Image, address: u64) -> Result<String> {
    string(|a, n| Ok(image.read(a, n, 1)?.to_vec()), address)
}
fn owned_string(object: &Object, slide: u64, section: &str, address: u64) -> Result<String> {
    let s = image_string(&object.image, address)?;
    let section = if section != "__objc_methname"
        && object.objc_sections.section(section, slide)?.is_none()
    {
        "__cstring"
    } else {
        section
    };
    object
        .objc_sections
        .contains(section, slide, address, s.len() + 1)?;
    Ok(s)
}
fn record<'a>(
    object: &'a Object,
    slide: u64,
    section: &str,
    address: u64,
    length: usize,
) -> Result<&'a [u8]> {
    if !address.is_multiple_of(8) {
        return Err("unaligned ObjC record".into());
    }
    object
        .objc_sections
        .contains(section, slide, address, length)?;
    object.image.read(address, length, 1)
}
fn patch(image: &mut Image, address: u64, value: u64) -> Result<()> {
    for segment in &mut image.segments {
        if segment.protection & 4 != 0 {
            continue;
        }
        if let Some(at) = address
            .checked_sub(segment.address)
            .and_then(|n| usize::try_from(n).ok())
            && let Some(end) = at.checked_add(8)
            && end <= segment.bytes.len()
        {
            std::sync::Arc::make_mut(&mut segment.bytes)[at..end]
                .copy_from_slice(&value.to_le_bytes());
            return Ok(());
        }
    }
    Err("ObjC patch outside data".into())
}
fn relative(field: u64, encoded: u32) -> Result<u64> {
    field
        .checked_add_signed(i64::from(encoded as i32))
        .ok_or("ObjC relative target overflow".into())
}
fn scalar_signature(name: &str, encoding: &str) -> Result<()> {
    let args = name.bytes().filter(|&b| b == b':').count();
    let expected = format!(
        "Q{}@0:8{}",
        16 + args * 8,
        (0..args)
            .map(|i| format!("Q{}", 16 + i * 8))
            .collect::<String>()
    );
    if args > 6 || encoding != expected || matches!(name, "load" | "initialize") {
        return Err("unsupported/duplicate ObjC scalar method".into());
    }
    Ok(())
}
fn methods(
    object: &Object,
    slide: u64,
    address: u64,
    selectors: &mut BTreeMap<String, u64>,
    patches: &mut Vec<(u64, String)>,
) -> Result<Vec<(String, u64)>> {
    if address == 0 {
        return Ok(Vec::new());
    }
    let compact = object
        .objc_sections
        .contains("__objc_methlist", slide, address, 8)
        .is_ok();
    let section = if compact {
        "__objc_methlist"
    } else {
        "__objc_const"
    };
    let alignment = if compact { 4 } else { 8 };
    if !address.is_multiple_of(alignment) {
        return Err("unaligned ObjC method list".into());
    }
    object.objc_sections.contains(section, slide, address, 8)?;
    let header = object.image.read(address, 8, 1)?;
    let count = u32_at(header, 4)? as usize;
    if u32_at(header, 0)? != if compact { 0x8000000c } else { 24 } || count > 128 {
        return Err("unsupported ObjC method list flags/count".into());
    }
    let stride = if compact { 12 } else { 24 };
    let length = 8 + count * stride;
    object
        .objc_sections
        .contains(section, slide, address, length)?;
    let data = object.image.read(address, length, 1)?;
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for n in 0..count {
        let at = 8 + n * stride;
        let entry = address + at as u64;
        let (slot, name_address, types, imp) = if compact {
            let slot = relative(entry, u32_at(data, at)?)?;
            if !slot.is_multiple_of(8) {
                return Err("unaligned ObjC relative selector slot".into());
            }
            object
                .objc_sections
                .contains("__objc_selrefs", slide, slot, 8)?;
            let (base, _) = object
                .objc_sections
                .section("__objc_selrefs", slide)?
                .unwrap();
            if !(slot - base).is_multiple_of(8) {
                return Err("ObjC relative selector not a complete slot".into());
            }
            (
                slot,
                u64_at(object.image.read(slot, 8, 1)?, 0)?,
                relative(entry + 4, u32_at(data, at + 4)?)?,
                relative(entry + 8, u32_at(data, at + 8)?)?,
            )
        } else {
            (
                entry,
                u64_at(data, at)?,
                u64_at(data, at + 8)?,
                u64_at(data, at + 16)?,
            )
        };
        let name = owned_string(object, slide, "__objc_methname", name_address)?;
        let encoding = owned_string(object, slide, "__objc_methtype", types)?;
        scalar_signature(&name, &encoding)?;
        if !seen.insert(name.clone()) {
            return Err("unsupported/duplicate ObjC scalar method".into());
        }
        if !object.image.contains_pc(imp) {
            return Err("ObjC IMP outside owning image code".into());
        }
        selectors
            .entry(name.clone())
            .and_modify(|v| *v = (*v).min(name_address))
            .or_insert(name_address);
        patches.push((slot, name.clone()));
        result.push((name, imp));
    }
    Ok(result)
}
struct Pending {
    address: u64,
    meta: u64,
    name: String,
    instance: Vec<(String, u64)>,
    class: Vec<(String, u64)>,
    protocols: Vec<u64>,
}

pub(crate) fn register(
    objects: &mut [Object],
    order: &[usize],
    slides: &[u64],
    runtime: Option<Runtime>,
) -> Result<Option<Plan>> {
    let active: Vec<_> = order
        .iter()
        .copied()
        .filter(|&i| !objects[i].objc_sections.is_empty())
        .collect();
    if active.is_empty() {
        return Ok(None);
    }
    let runtime = runtime.ok_or("ObjC requires explicit libobjc runtime")?;
    let mut selectors = BTreeMap::new();
    let mut patches: Vec<(usize, u64, String)> = Vec::new();
    let mut pending = Vec::new();
    let mut names = BTreeSet::new();
    let mut addresses = BTreeSet::new();
    let mut total: usize = active
        .iter()
        .map(|&i| {
            objects[i]
                .objc_sections
                .parts
                .iter()
                .map(|p| p.length)
                .sum::<usize>()
        })
        .sum();
    if total > CAP {
        return Err("aggregate ObjC metadata exceeds cap".into());
    }
    let protocols = protocol::register(
        objects,
        &active,
        slides,
        runtime.protocols,
        &mut selectors,
        &mut patches,
        &mut total,
    )?;
    for &index in &active {
        let object = &objects[index];
        let slide = slides[index];
        if !object
            .dependencies
            .iter()
            .any(|(n, _)| n == "/usr/lib/libobjc.A.dylib")
        {
            return Err("ObjC image lacks libobjc dependency".into());
        }
        let (info, len) = object
            .objc_sections
            .section("__objc_imageinfo", slide)?
            .ok_or("ObjC imageinfo required")?;
        let bytes = object.image.read(info, len, 1)?;
        if u32_at(bytes, 0)? != 0 || u32_at(bytes, 4)? != 0x40 {
            return Err("unsupported ObjC image flags".into());
        }
        if let Some((base, len)) = object.objc_sections.section("__objc_classlist", slide)? {
            for at in (0..len).step_by(8) {
                let address = u64_at(object.image.read(base + at as u64, 8, 1)?, 0)?;
                let class = record(object, slide, "__objc_data", address, 40)?;
                let meta = u64_at(class, 0)?;
                let metaclass = record(object, slide, "__objc_data", meta, 40)?;
                if address == meta
                    || !addresses.insert(address)
                    || !addresses.insert(meta)
                    || addresses.len() > 128
                    || u64_at(class, 8)? != 0
                    || u64_at(metaclass, 0)? != meta
                    || u64_at(metaclass, 8)? != address
                {
                    return Err("unsupported ObjC root class graph".into());
                }
                let mut tables = Vec::new();
                let mut adoption = None;
                let mut class_name = None;
                for (words, is_meta) in [(class, false), (metaclass, true)] {
                    if u64_at(words, 16)? != runtime.empty_cache || u64_at(words, 24)? != 0 {
                        return Err("unsupported ObjC cache/vtable".into());
                    }
                    let ro = record(object, slide, "__objc_const", u64_at(words, 32)?, 72)?;
                    if u32_at(ro, 0)? != if is_meta { 3 } else { 2 }
                        || u32_at(ro, 4)? != if is_meta { 40 } else { 0 }
                        || u32_at(ro, 8)? != if is_meta { 40 } else { 8 }
                        || u32_at(ro, 12)? != 0
                        || [16, 56, 64].iter().any(|&at| u64_at(ro, at).unwrap() != 0)
                    {
                        return Err("unsupported ObjC class RO layout/flags".into());
                    }
                    let name = owned_string(object, slide, "__objc_classname", u64_at(ro, 24)?)?;
                    if let Some(old) = &class_name {
                        if old != &name {
                            return Err("ObjC metaclass name mismatch".into());
                        }
                    } else {
                        class_name = Some(name);
                    }
                    let list = u64_at(ro, 40)?;
                    if let Some((old, _)) = &adoption {
                        if *old != list {
                            return Err("class/metaclass protocol adoption mismatch".into());
                        }
                    } else {
                        adoption = Some((list, protocols.adoption(object, slide, list)?));
                    }
                    let ivars = u64_at(ro, 48)?;
                    if is_meta && ivars != 0 {
                        return Err("ObjC metaclass ivars unsupported".into());
                    }
                    if !is_meta && ivars != 0 {
                        let data = record(object, slide, "__objc_const", ivars, 40)?;
                        let cell = u64_at(data, 8)?;
                        object
                            .objc_sections
                            .contains("__objc_ivar", slide, cell, 4)?;
                        if u32_at(data, 0)? != 32
                            || u32_at(data, 4)? != 1
                            || cell % 4 != 0
                            || u32_at(object.image.read(cell, 4, 1)?, 0)? != 0
                            || owned_string(object, slide, "__objc_methname", u64_at(data, 16)?)?
                                != "isa"
                            || owned_string(object, slide, "__objc_methtype", u64_at(data, 24)?)?
                                != "#"
                            || u32_at(data, 32)? != 3
                            || u32_at(data, 36)? != 8
                        {
                            return Err("unsupported ObjC ivar layout".into());
                        }
                    }
                    let mut local = Vec::new();
                    tables.push(methods(
                        object,
                        slide,
                        u64_at(ro, 32)?,
                        &mut selectors,
                        &mut local,
                    )?);
                    patches.extend(local.into_iter().map(|(a, n)| (index, a, n)));
                }
                let name = class_name.unwrap();
                if !names.insert(name.clone()) {
                    return Err("duplicate ObjC class name".into());
                }
                let class = tables.pop().unwrap();
                let instance = tables.pop().unwrap();
                pending.push(Pending {
                    address,
                    meta,
                    name,
                    instance,
                    class,
                    protocols: adoption.unwrap().1,
                });
            }
        }
        if let Some((base, len)) = object.objc_sections.section("__objc_selrefs", slide)? {
            for at in (0..len).step_by(8) {
                let slot = base + at as u64;
                let address = u64_at(object.image.read(slot, 8, 1)?, 0)?;
                let name = owned_string(object, slide, "__objc_methname", address)?;
                selectors
                    .entry(name.clone())
                    .and_modify(|v| *v = (*v).min(address))
                    .or_insert(address);
                patches.push((index, slot, name));
            }
        }
    }
    if selectors.len() > 1024
        || pending
            .iter()
            .map(|p| p.instance.len() + p.class.len())
            .sum::<usize>()
            > 1024
    {
        return Err("ObjC selectors/methods exceed cap".into());
    }
    for &index in &active {
        let object = &objects[index];
        if let Some((base, len)) = object
            .objc_sections
            .section("__objc_classrefs", slides[index])?
        {
            for at in (0..len).step_by(8) {
                let target = u64_at(object.image.read(base + at as u64, 8, 1)?, 0)?;
                if !pending.iter().any(|p| p.address == target) {
                    return Err("unregistered ObjC classref".into());
                }
            }
        }
    }
    for (index, address, name) in patches {
        patch(&mut objects[index].image, address, selectors[&name])?;
    }
    let mut snapshots = Vec::new();
    for index in active {
        let object = &objects[index];
        for section in &object.objc_sections.parts {
            let address = section
                .address
                .checked_add(slides[index])
                .ok_or("ObjC snapshot overflow")?;
            snapshots.push((
                address,
                object.image.read(address, section.length, 1)?.to_vec(),
            ));
        }
    }
    let convert = |v: Vec<(String, u64)>| {
        v.into_iter()
            .map(|(n, imp)| Method {
                selector: selectors[&n],
                imp,
            })
            .collect()
    };
    let classes = pending
        .into_iter()
        .map(|p| Class {
            address: p.address,
            meta: p.meta,
            name: p.name,
            instance: convert(p.instance),
            class: convert(p.class),
            protocols: p.protocols,
        })
        .collect();
    Ok(Some(Plan {
        classes,
        selectors: selectors.into_values().collect(),
        snapshots,
        runtime,
        protocols,
    }))
}
impl Plan {
    pub(crate) fn validate(&self, image: &Image) -> Result<()> {
        self.protocols.validate(image)?;
        for (address, bytes) in &self.snapshots {
            metadata_not_code(*address, bytes.len(), image.code_ranges())?;
            if image.read(*address, bytes.len(), 1)? != bytes {
                return Err("prepared ObjC metadata mismatch".into());
            }
        }
        let r = self.runtime;
        for (base, words) in [
            (r.get_class, vec![GET_CLASS, RET]),
            (
                r.msg_send,
                vec![0xb4000060, LOOKUP, BR_X16, 0xd2800000, RET],
            ),
        ] {
            for (i, word) in words.into_iter().enumerate() {
                if image.instruction(
                    base.checked_add(i as u64 * 4)
                        .ok_or("ObjC runtime overflow")?,
                )? != word
                {
                    return Err("unsupported ObjC runtime stub".into());
                }
            }
        }
        if image.read(r.empty_cache, 8, 1)? != [0; 8] {
            return Err("unsupported ObjC empty cache".into());
        }
        for class in &self.classes {
            for method in class.instance.iter().chain(&class.class) {
                if !image.contains_pc(method.imp) {
                    return Err("prepared ObjC IMP outside code".into());
                }
            }
        }
        Ok(())
    }
    pub(crate) fn trap(
        &self,
        memory: &Memory,
        trap: u64,
        word: u32,
        x0: u64,
        x1: u64,
    ) -> Result<u64> {
        if !((word == GET_CLASS && trap == self.runtime.get_class)
            || (word == LOOKUP && trap == self.runtime.msg_send + 4)
            || self.protocols.registered_trap(trap, word))
        {
            return Err("unregistered ObjC trap".into());
        }
        for (address, bytes) in &self.snapshots {
            if memory.read(*address, bytes.len())? != bytes {
                return Err("mutated ObjC metadata unsupported".into());
            }
        }
        self.protocols.validate_memory(memory)?;
        if memory.read(self.runtime.empty_cache, 8)? != [0; 8] {
            return Err("mutated ObjC cache unsupported".into());
        }
        if word == protocol::GET {
            return self.protocols.lookup(memory, x0);
        }
        if word == protocol::NAME {
            return self.protocols.name(x0);
        }
        if word == protocol::CONFORMS {
            if x0 == 0 || x1 == 0 {
                return Ok(0);
            }
            self.protocols.contains(x1)?;
            let class = self
                .classes
                .iter()
                .find(|c| c.address == x0)
                .ok_or("unregistered protocol class identity")?;
            return Ok(u64::from(class.protocols.contains(&x1)));
        }
        if word == GET_CLASS {
            let name = string(|a, n| Ok(memory.read(a, n)?.to_vec()), x0)?;
            return Ok(self
                .classes
                .iter()
                .find(|c| c.name == name)
                .map_or(0, |c| c.address));
        }
        if !self.selectors.contains(&x1) {
            return Err("unregistered ObjC selector".into());
        }
        if self.classes.iter().any(|c| c.meta == x0) {
            return Err("ObjC metaclass receiver unsupported".into());
        }
        let methods = if let Some(class) = self.classes.iter().find(|c| c.address == x0) {
            &class.class
        } else {
            if x0 == 0 || !x0.is_multiple_of(8) {
                return Err("unsupported ObjC receiver".into());
            }
            let isa = u64_at(memory.read(x0, 8)?, 0)?;
            &self
                .classes
                .iter()
                .find(|c| c.address == isa)
                .ok_or("unregistered ObjC raw isa")?
                .instance
        };
        methods
            .iter()
            .find(|m| m.selector == x1)
            .map(|m| m.imp)
            .ok_or("unknown ObjC selector for receiver".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn loaded() -> (Object, Runtime) {
        loaded_profile("objc")
    }
    fn loaded_profile(profile: &str) -> (Object, Runtime) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/bin")
            .join(profile)
            .join("chained");
        let raw = std::fs::read(root.join("objc-0.macho")).unwrap();
        let libs = [
            "libsable_other.dylib",
            "libsable_objc.dylib",
            "libsable_system.dylib",
        ]
        .map(|n| std::fs::read(root.join(n)).unwrap());
        let linked =
            Image::link(&raw, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
        let runtime = linked.objc.as_ref().unwrap().runtime;
        let mut object = Object::parse(&raw, true).unwrap();
        // Preserve this object's own code ranges and segment ownership, but use
        // its actual fully fixed-up bytes. No synthetic metadata format here.
        for s in &mut object.image.segments {
            *s = linked
                .segments
                .iter()
                .find(|p| p.address == s.address)
                .unwrap()
                .clone();
        }
        (object, runtime)
    }
    fn word(object: &Object, address: u64) -> u64 {
        u64_at(object.image.read(address, 8, 1).unwrap(), 0).unwrap()
    }
    #[test]
    fn plain_c_string_section_keeps_the_existing_image_limit() {
        let mut section = [0u8; 80];
        section[..16].copy_from_slice(b"__cstring\0\0\0\0\0\0\0");
        section[32..40].copy_from_slice(&0x1000u64.to_le_bytes());
        section[40..48].copy_from_slice(&((CAP + 1) as u64).to_le_bytes());
        section[64..68].copy_from_slice(&2u32.to_le_bytes());
        let mut sections = Sections::default();
        sections
            .push(
                &section,
                (0x1000, (CAP + 1) as u64, (CAP + 1) as u64, 0, 1),
                true,
            )
            .unwrap();
        assert!(sections.is_empty()); // No ObjC registration or ObjC cap for C-only images.
        let mut object = loaded().0;
        object
            .objc_sections
            .parts
            .iter_mut()
            .find(|p| p.name == "__cstring")
            .unwrap()
            .length = CAP + 1;
        let runtime = loaded().1;
        assert!(
            register(&mut [object], &[0], &[0], Some(runtime))
                .unwrap_err()
                .contains("aggregate ObjC metadata exceeds cap")
        );
    }
    #[test]
    fn malformed_real_root_graph_ro_methods_ivars_and_owning_imp_reject() {
        let (mut object, runtime) = loaded();
        assert!(
            register(std::slice::from_mut(&mut object), &[0], &[0], Some(runtime))
                .unwrap()
                .is_some()
        );
        let base = object
            .objc_sections
            .section("__objc_classlist", 0)
            .unwrap()
            .unwrap()
            .0;
        let class = word(&object, base);
        let meta = word(&object, class);
        let ro = word(&object, class + 32);
        let meta_ro = word(&object, meta + 32);
        let methods = word(&object, ro + 32);
        let ivars = word(&object, ro + 48);
        let offset_cell = word(&object, ivars + 8);
        let types = word(&object, methods + 16);
        let mut changes = vec![
            (class, class),
            (class + 8, 1),
            (class + 16, 0),
            (class + 24, 1),
            (class + 32, ro | 1),
            (meta, class),
            (meta + 8, 0),
            (ro + 16, 1),
            (ro + 40, 1),
            (ro + 56, 1),
            (ro + 64, 1),
            (methods + 24, runtime.get_class),
        ];
        // Duplicate method identity is invalid even if both IMPs are valid code.
        changes.push((methods + 32, word(&object, methods + 8)));
        for (address, value) in changes {
            let (mut bad, runtime) = loaded();
            patch(&mut bad.image, address, value).unwrap();
            assert!(
                register(&mut [bad], &[0], &[0], Some(runtime)).is_err(),
                "accepted {address:#x}={value:#x}"
            );
        }
        for (address, value) in [
            (ro, 3),
            (ro + 4, 8),
            (ro + 8, 16),
            (ro + 12, 1),
            (meta_ro, 2),
            (meta_ro + 8, 8),
            (methods, 0x80000018),
            (methods + 4, u32::MAX),
            (ivars, 24),
            (ivars + 4, 2),
            (ivars + 36, 4),
            (offset_cell, 8),
        ] {
            let (mut bad, runtime) = loaded();
            for s in &mut bad.image.segments {
                if let Some(at) = address.checked_sub(s.address).map(|a| a as usize)
                    && at + 4 <= s.bytes.len()
                {
                    std::sync::Arc::make_mut(&mut s.bytes)[at..at + 4]
                        .copy_from_slice(&value.to_le_bytes());
                    break;
                }
            }
            assert!(
                register(&mut [bad], &[0], &[0], Some(runtime)).is_err(),
                "accepted {address:#x}={value:#x}"
            );
        }
        let (mut bad, runtime) = loaded();
        for s in &mut bad.image.segments {
            if let Some(at) = types.checked_sub(s.address).map(|a| a as usize)
                && at < s.bytes.len()
            {
                std::sync::Arc::make_mut(&mut s.bytes)[at] = b'q';
                break;
            }
        }
        assert!(
            register(&mut [bad], &[0], &[0], Some(runtime))
                .unwrap_err()
                .contains("scalar method")
        );
        let (bad, _) = loaded();
        assert!(register(&mut [bad], &[0], &[0], None).is_err());
    }
    fn overwrite(object: &mut Object, address: u64, bytes: &[u8]) {
        for s in &mut object.image.segments {
            if let Some(at) = address
                .checked_sub(s.address)
                .and_then(|a| usize::try_from(a).ok())
                && at
                    .checked_add(bytes.len())
                    .is_some_and(|end| end <= s.bytes.len())
            {
                std::sync::Arc::make_mut(&mut s.bytes)[at..at + bytes.len()].copy_from_slice(bytes);
                return;
            }
        }
        panic!("test write out of range");
    }
    #[test]
    fn compact_field_relative_math_and_malformed_real_records_reject() {
        assert_eq!(relative(0x1000, (-4i32) as u32).unwrap(), 0xffc);
        assert_eq!(relative(0x1000, 0).unwrap(), 0x1000);
        assert!(relative(0, (-1i32) as u32).is_err());
        assert!(relative(u64::MAX, 1).is_err());
        let (mut original, runtime) = loaded_profile("objc-compact");
        assert!(
            register(
                std::slice::from_mut(&mut original),
                &[0],
                &[0],
                Some(runtime)
            )
            .unwrap()
            .is_some()
        );
        let classlist = original
            .objc_sections
            .section("__objc_classlist", 0)
            .unwrap()
            .unwrap()
            .0;
        let class = word(&original, classlist);
        let ro = word(&original, class + 32);
        let methods = word(&original, ro + 32);
        let (slots, _) = original
            .objc_sections
            .section("__objc_selrefs", 0)
            .unwrap()
            .unwrap();
        let entry = methods + 8;
        let target = |field: u64, at: u64| (at as i64 - field as i64) as i32 as u32;
        let mut changes = vec![
            (methods, 24),
            (methods, 0xc000000c),
            (methods, 0x80000018),
            (methods, 0x8001000c),
            (methods + 4, 129),
            (methods + 4, u32::MAX),
        ];
        for field in [entry, entry + 4, entry + 8] {
            for value in [0, i32::MAX as u32, i32::MIN as u32] {
                changes.push((field, value));
            }
        }
        for slot in [slots + 1, slots + 4, methods, runtime.empty_cache] {
            changes.push((entry, target(entry, slot)));
        }
        for imp in [runtime.get_class, methods, original.image.entry + 1] {
            changes.push((entry + 8, target(entry + 8, imp)));
        }
        // Treating the entry base as all fields' base must not accidentally succeed.
        let types = relative(
            entry + 4,
            u32_at(original.image.read(entry + 4, 4, 1).unwrap(), 0).unwrap(),
        )
        .unwrap();
        let imp = relative(
            entry + 8,
            u32_at(original.image.read(entry + 8, 4, 1).unwrap(), 0).unwrap(),
        )
        .unwrap();
        changes.push((entry + 4, target(entry, types)));
        let mut selectors = BTreeMap::new();
        let mut patches = Vec::new();
        assert_eq!(
            super::methods(&original, 0, methods, &mut selectors, &mut patches).unwrap()[0].1,
            imp
        );
        // A wrong source-level target may still be aligned owning code. Decode
        // its actual field-relative meaning, rather than rejecting valid code.
        let (mut alternate, _) = loaded_profile("objc-compact");
        overwrite(&mut alternate, entry + 8, &target(entry, imp).to_le_bytes());
        assert_eq!(
            super::methods(
                &alternate,
                0,
                methods,
                &mut BTreeMap::new(),
                &mut Vec::new()
            )
            .unwrap()[0]
                .1,
            imp + 8
        );
        for (address, value) in changes {
            let (mut object, runtime) = loaded_profile("objc-compact");
            overwrite(&mut object, address, &value.to_le_bytes());
            assert!(
                register(&mut [object], &[0], &[0], Some(runtime)).is_err(),
                "accepted compact mutation {address:#x}={value:#x}"
            );
        }
        for delta in [1, 2, 3] {
            let (mut object, runtime) = loaded_profile("objc-compact");
            overwrite(&mut object, ro + 32, &(methods + delta).to_le_bytes());
            assert!(register(&mut [object], &[0], &[0], Some(runtime)).is_err());
        }
        for length in [4, 8, 19] {
            let (mut object, runtime) = loaded_profile("objc-compact");
            object
                .objc_sections
                .parts
                .iter_mut()
                .find(|s| s.name == "__objc_methlist")
                .unwrap()
                .length = length;
            assert!(register(&mut [object], &[0], &[0], Some(runtime)).is_err());
        }
    }
}
