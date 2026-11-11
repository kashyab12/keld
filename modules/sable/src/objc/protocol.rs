//! Original bounded static protocol metadata and direct introspection.
use super::*;

pub(crate) const GET: u32 = 0xd4000001 | (0x5ab4 << 5);
pub(crate) const NAME: u32 = 0xd4000001 | (0x5ab5 << 5);
pub(crate) const CONFORMS: u32 = 0xd4000001 | (0x5ab6 << 5);
pub(crate) fn is_service(word: u32) -> bool {
    matches!(word, GET | NAME | CONFORMS)
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Runtime {
    pub get: u64,
    pub name: u64,
    pub conforms: u64,
    pub nil_name: u64,
}
#[derive(Debug)]
pub(super) struct DataSection {
    header: [u8; 80],
    segment: (u64, u64, u64, u64, u32),
}
impl DataSection {
    pub(super) fn new(header: &[u8], segment: (u64, u64, u64, u64, u32)) -> Self {
        Self {
            header: header.try_into().unwrap(),
            segment,
        }
    }
    fn contains(&self, slide: u64, address: u64, length: usize) -> Result<bool> {
        let s = &self.header;
        let (base, _, file_size, file_offset, protection) = self.segment;
        let start = u64_at(s, 32)?;
        let len = u64_at(s, 40)?;
        let end = start
            .checked_add(len)
            .ok_or("protocol data section overflow")?;
        let shifted = start
            .checked_add(slide)
            .ok_or("protocol data slide overflow")?;
        if address
            .checked_sub(shifted)
            .and_then(|v| v.checked_add(length as u64))
            .is_none_or(|end| end > len)
        {
            return Ok(false);
        }
        let align = u32_at(s, 52)?;
        if protection != 3
            || u32_at(s, 64)? & 255 != 0
            || align > 12
            || start % (1u64 << align) != 0
            || start < base
            || end
                > base
                    .checked_add(file_size)
                    .ok_or("protocol data file range overflow")?
            || u64::from(u32_at(s, 48)?)
                != file_offset
                    .checked_add(start - base)
                    .ok_or("protocol file offset overflow")?
        {
            return Err("invalid owning protocol data section".into());
        }
        Ok(true)
    }
}
#[derive(Clone, Debug)]
struct Definition {
    address: u64,
    name_address: u64,
    name: String,
}
#[derive(Clone, Debug)]
pub(super) struct Plan {
    definitions: Vec<Definition>,
    bodies: Vec<(u64, Vec<u8>)>,
    runtime: Option<Runtime>,
}

fn body(object: &Object, slide: u64, address: u64) -> Result<&[u8]> {
    if object.objc_sections.data_overflow {
        return Err("protocol data ownership exceeds cap".into());
    }
    if !address.is_multiple_of(8) {
        return Err("unaligned protocol body".into());
    }
    let mut owners = 0;
    for data in &object.objc_sections.ordinary_data {
        if data.contains(slide, address, 96)? {
            owners += 1;
        }
    }
    if owners != 1 {
        return Err("protocol body outside unique owning __data".into());
    }
    let end = address.checked_add(96).ok_or("protocol body overflow")?;
    for part in &object.objc_sections.parts {
        let base = part
            .address
            .checked_add(slide)
            .ok_or("protocol metadata slide overflow")?;
        if address
            < base
                .checked_add(part.length as u64)
                .ok_or("protocol metadata overflow")?
            && base < end
        {
            return Err("protocol body overlaps other ObjC metadata".into());
        }
    }
    metadata_not_code(address, 96, object.image.code_ranges())?;
    object.image.read(address, 96, 3)
}

pub(super) fn register(
    objects: &[Object],
    active: &[usize],
    slides: &[u64],
    runtime: Option<Runtime>,
    selectors: &mut BTreeMap<String, u64>,
    patches: &mut Vec<(usize, u64, String)>,
    total: &mut usize,
) -> Result<Plan> {
    let mut definitions = Vec::new();
    let mut bodies = Vec::new();
    let mut names = BTreeSet::new();
    let mut intervals = Vec::new();
    let mut parents = Vec::new();
    for &index in active {
        let object = &objects[index];
        let slide = slides[index];
        if let Some((base, len)) = object.objc_sections.section("__objc_protolist", slide)? {
            if runtime.is_none() {
                return Err("protocols require explicit protocol runtime exports".into());
            }
            for at in (0..len).step_by(8) {
                let address = u64_at(object.image.read(base + at as u64, 8, 1)?, 0)?;
                let data = body(object, slide, address)?;
                let end = address.checked_add(96).ok_or("protocol range overflow")?;
                if intervals.iter().any(|&(a, b)| address < b && a < end) || definitions.len() >= 64
                {
                    return Err("overlapping/duplicate/excessive protocols".into());
                }
                intervals.push((address, end));
                // Clang's protocol_t: isa 0, no Swift demangled name, size 96, flags 0.
                if u32_at(data, 64)? != 96
                    || u32_at(data, 68)? != 0
                    || [0, 80].iter().any(|&n| u64_at(data, n).unwrap() != 0)
                {
                    return Err("unsupported protocol body size/flags/fields".into());
                }
                let name_address = u64_at(data, 8)?;
                let name = owned_string(object, slide, "__objc_classname", name_address)?;
                if !names.insert(name.clone()) {
                    return Err("duplicate protocol name unsupported".into());
                }
                let inherited = u64_at(data, 16)?;
                if inherited != 0 {
                    let count = u64_at(record(object, slide, "__objc_const", inherited, 8)?, 0)?;
                    if count == 0 || count > 16 {
                        return Err("unsupported inherited protocol list".into());
                    }
                    let rows = record(
                        object,
                        slide,
                        "__objc_const",
                        inherited,
                        8 + count as usize * 8,
                    )?;
                    for n in 0..count as usize {
                        parents.push((address, u64_at(rows, 8 + n * 8)?));
                    }
                }
                // Required/optional instance/class descriptions share one extended-types array.
                let extended = u64_at(data, 72)?;
                let mut lists = Vec::new();
                for offset in [24, 32, 40, 48] {
                    let list = u64_at(data, offset)?;
                    if list == 0 {
                        continue;
                    }
                    let compact = object
                        .objc_sections
                        .contains("__objc_methlist", slide, list, 8)
                        .is_ok();
                    let (section, flags, align) = if compact {
                        ("__objc_methlist", 0x8000_000c, 4)
                    } else {
                        ("__objc_const", 24, 8)
                    };
                    if !list.is_multiple_of(align) {
                        return Err("unaligned protocol descriptions".into());
                    }
                    object.objc_sections.contains(section, slide, list, 8)?;
                    let header = object.image.read(list, 8, 1)?;
                    let count = u32_at(header, 4)? as usize;
                    if u32_at(header, 0)? != flags || count == 0 || count > 128 {
                        return Err("unsupported protocol descriptions".into());
                    }
                    lists.push((list, count, compact));
                }
                let described: usize = lists.iter().map(|(_, n, _)| n).sum();
                if described == 0 && extended != 0 {
                    return Err("protocol extended types without descriptions".into());
                }
                let types = if described == 0 {
                    &[][..]
                } else {
                    record(object, slide, "__objc_const", extended, described * 8)?
                };
                let mut seen = BTreeSet::new();
                let mut type_index = 0;
                for (list, count, compact) in lists {
                    let stride = if compact { 12 } else { 24 };
                    let section = if compact {
                        "__objc_methlist"
                    } else {
                        "__objc_const"
                    };
                    object
                        .objc_sections
                        .contains(section, slide, list, 8 + count * stride)?;
                    let rows = object.image.read(list, 8 + count * stride, 1)?;
                    for n in 0..count {
                        let offset = 8 + n * stride;
                        let entry = list + offset as u64;
                        let (selector_address, type_address, no_imp) = if compact {
                            let slot = relative(entry, u32_at(rows, offset)?)?;
                            let (base, _) = object
                                .objc_sections
                                .section("__objc_selrefs", slide)?
                                .ok_or("relative protocol selector without selrefs")?;
                            object
                                .objc_sections
                                .contains("__objc_selrefs", slide, slot, 8)?;
                            if !slot.is_multiple_of(8) || !(slot - base).is_multiple_of(8) {
                                return Err("protocol selector not a complete selref slot".into());
                            }
                            (
                                u64_at(object.image.read(slot, 8, 1)?, 0)?,
                                relative(entry + 4, u32_at(rows, offset + 4)?)?,
                                u32_at(rows, offset + 8)? == 0,
                            )
                        } else {
                            (
                                u64_at(rows, offset)?,
                                u64_at(rows, offset + 8)?,
                                u64_at(rows, offset + 16)? == 0,
                            )
                        };
                        if !no_imp || type_address == 0 {
                            return Err("protocol description IMP/types mismatch".into());
                        }
                        let selector =
                            owned_string(object, slide, "__objc_methname", selector_address)?;
                        // Descriptions are metadata only and never dispatched, so the
                        // plain and extended encodings are bounded ASCII, not type-walked.
                        owned_string(object, slide, "__objc_methtype", type_address)?;
                        image_string(&object.image, u64_at(types, type_index * 8)?)?;
                        type_index += 1;
                        if !seen.insert(selector.clone()) {
                            return Err("duplicate protocol description".into());
                        }
                        selectors
                            .entry(selector.clone())
                            .and_modify(|v| *v = (*v).min(selector_address))
                            .or_insert(selector_address);
                        if !compact {
                            patches.push((index, entry, selector));
                        }
                    }
                }
                for offset in [56, 88] {
                    let list = u64_at(data, offset)?;
                    if list == 0 {
                        continue;
                    }
                    let header = record(object, slide, "__objc_const", list, 8)?;
                    let count = u32_at(header, 4)? as usize;
                    if u32_at(header, 0)? != 16 || count == 0 || count > 64 {
                        return Err("unsupported protocol property list".into());
                    }
                    let rows = record(object, slide, "__objc_const", list, 8 + count * 16)?;
                    for n in 0..count * 2 {
                        image_string(&object.image, u64_at(rows, 8 + n * 8)?)?;
                    }
                }
                *total = total
                    .checked_add(96)
                    .ok_or("protocol metadata cap overflow")?;
                if *total > CAP {
                    return Err("aggregate ObjC protocol metadata exceeds cap".into());
                }
                bodies.push((address, data.to_vec()));
                definitions.push(Definition {
                    address,
                    name_address,
                    name,
                });
            }
        }
    }
    let plan = Plan {
        definitions,
        bodies,
        runtime,
    };
    for (child, parent) in parents {
        if child == parent {
            return Err("protocol inherits itself".into());
        }
        plan.definition(parent)?;
    }
    for &index in active {
        let object = &objects[index];
        if let Some((base, len)) = object
            .objc_sections
            .section("__objc_protorefs", slides[index])?
        {
            if plan.runtime.is_none() {
                return Err("protocol references require protocol runtime".into());
            }
            for at in (0..len).step_by(8) {
                plan.definition(u64_at(object.image.read(base + at as u64, 8, 1)?, 0)?)?;
            }
        }
    }
    Ok(plan)
}
impl Plan {
    fn definition(&self, address: u64) -> Result<&Definition> {
        self.definitions
            .iter()
            .find(|p| p.address == address)
            .ok_or("unregistered protocol identity".into())
    }
    pub(super) fn adoption(&self, object: &Object, slide: u64, address: u64) -> Result<Vec<u64>> {
        if address == 0 {
            return Ok(Vec::new());
        }
        let count = u64_at(record(object, slide, "__objc_const", address, 8)?, 0)?;
        if count > 64 {
            return Err("protocol adoption count exceeds cap".into());
        }
        let rows = record(
            object,
            slide,
            "__objc_const",
            address,
            8 + count as usize * 8,
        )?;
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        for n in 0..count as usize {
            let protocol = u64_at(rows, 8 + n * 8)?;
            self.definition(protocol)?;
            if !seen.insert(protocol) {
                return Err("duplicate protocol adoption".into());
            }
            result.push(protocol);
        }
        Ok(result)
    }
    pub(super) fn validate(&self, image: &Image) -> Result<()> {
        for (address, bytes) in &self.bodies {
            metadata_not_code(*address, bytes.len(), image.code_ranges())?;
            if image.read(*address, bytes.len(), 3)? != bytes {
                return Err("prepared protocol body mismatch".into());
            }
        }
        if let Some(r) = self.runtime {
            for (address, word) in [(r.get, GET), (r.name, NAME), (r.conforms, CONFORMS)] {
                if image.instruction(address)? != word
                    || image.instruction(address.checked_add(4).ok_or("protocol stub overflow")?)?
                        != RET
                {
                    return Err("invalid protocol runtime stub".into());
                }
            }
            metadata_not_code(r.nil_name, 4, image.code_ranges())?;
            if image.read(r.nil_name, 4, 1)? != b"nil\0" {
                return Err("invalid protocol nil-name constant".into());
            }
        }
        Ok(())
    }
    pub(super) fn validate_memory(&self, memory: &Memory) -> Result<()> {
        for (address, bytes) in &self.bodies {
            if memory.read(*address, bytes.len())? != bytes {
                return Err("mutated protocol body unsupported".into());
            }
        }
        if let Some(r) = self.runtime
            && memory.read(r.nil_name, 4)? != b"nil\0"
        {
            return Err("mutated protocol nil-name constant".into());
        }
        Ok(())
    }
    pub(super) fn registered_trap(&self, trap: u64, word: u32) -> bool {
        self.runtime.is_some_and(|r| {
            matches!((trap,word),(a,GET) if a==r.get)
                || matches!((trap,word),(a,NAME) if a==r.name)
                || matches!((trap,word),(a,CONFORMS) if a==r.conforms)
        })
    }
    pub(super) fn lookup(&self, memory: &Memory, address: u64) -> Result<u64> {
        let name = string(|a, n| Ok(memory.read(a, n)?.to_vec()), address)?;
        Ok(self
            .definitions
            .iter()
            .find(|p| p.name == name)
            .map_or(0, |p| p.address))
    }
    pub(super) fn name(&self, address: u64) -> Result<u64> {
        if address == 0 {
            return self
                .runtime
                .map(|r| r.nil_name)
                .ok_or("protocol runtime absent".into());
        }
        Ok(self.definition(address)?.name_address)
    }
    pub(super) fn contains(&self, address: u64) -> Result<()> {
        self.definition(address).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn loaded() -> (Object, super::super::Runtime) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/bin/protocols/absolute/chained");
        let raw = std::fs::read(root.join("protocol-0.macho")).unwrap();
        let libs = ["libsable_objc.dylib", "libsable_system.dylib"]
            .map(|n| std::fs::read(root.join(n)).unwrap());
        let image = Image::link(&raw, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
        let runtime = image.objc.as_ref().unwrap().runtime;
        let mut object = Object::parse(&raw, true).unwrap();
        for s in &mut object.image.segments {
            *s = image
                .segments
                .iter()
                .find(|p| p.address == s.address)
                .unwrap()
                .clone();
        }
        (object, runtime)
    }
    fn word(object: &Object, at: u64) -> u64 {
        u64_at(object.image.read(at, 8, 1).unwrap(), 0).unwrap()
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
        panic!("bad test patch");
    }
    fn rejects(mut object: Object, runtime: super::super::Runtime) {
        assert!(
            crate::objc::register(std::slice::from_mut(&mut object), &[0], &[0], Some(runtime))
                .is_err()
        );
    }
    #[test]
    fn malformed_real_protocol_bodies_descriptions_types_and_refs_reject() {
        let (object, runtime) = loaded();
        let (list, len) = object
            .objc_sections
            .section("__objc_protolist", 0)
            .unwrap()
            .unwrap();
        assert_eq!(len, 16);
        let a = word(&object, list);
        let b = word(&object, list + 8);
        let descriptions = word(&object, a + 24);
        let extended = word(&object, a + 72);
        for offset in [0, 16, 32, 40, 48, 56, 80, 88] {
            let (mut bad, r) = loaded();
            overwrite(&mut bad, a + offset, &a.to_le_bytes());
            rejects(bad, r);
        }
        for (address, value) in [
            (a + 64, 88u32),
            (a + 64, 104),
            (a + 68, 1),
            (descriptions, 12),
            (descriptions, 0x80000018),
            (descriptions + 4, 129),
            (descriptions + 4, u32::MAX),
        ] {
            let (mut bad, r) = loaded();
            overwrite(&mut bad, address, &value.to_le_bytes());
            rejects(bad, r);
        }
        for (address, value) in [
            (a + 8, 0),
            (a + 24, u64::MAX),
            (a + 72, 0),
            (descriptions + 8, 0),
            (descriptions + 16, 0),
            (descriptions + 24, runtime.get_class),
            (extended, 0),
            (list, a + 8),
            (list, 0),
        ] {
            let (mut bad, r) = loaded();
            overwrite(&mut bad, address, &value.to_le_bytes());
            rejects(bad, r);
        }
        let (mut bad, r) = loaded();
        overwrite(&mut bad, b + 8, &word(&object, a + 8).to_le_bytes());
        rejects(bad, r);
        let (mut bad, r) = loaded();
        overwrite(&mut bad, list + 8, &(a + 8).to_le_bytes());
        rejects(bad, r);
        let (refs, _) = object
            .objc_sections
            .section("__objc_protorefs", 0)
            .unwrap()
            .unwrap();
        for pointer in [a + 8, 0, u64::MAX, runtime.get_class] {
            let (mut bad, r) = loaded();
            overwrite(&mut bad, refs, &pointer.to_le_bytes());
            rejects(bad, r);
        }
        let (bad, mut r) = loaded();
        r.protocols = None;
        rejects(bad, r);
    }
    #[test]
    fn ordinary_data_is_not_blanket_metadata_and_protocol_body_cannot_alias_sections() {
        let mut header = [0u8; 80];
        header[..16].copy_from_slice(b"__data\0\0\0\0\0\0\0\0\0\0");
        header[32..40].copy_from_slice(&0x1000u64.to_le_bytes());
        header[40..48].copy_from_slice(&((CAP + 1) as u64).to_le_bytes());
        let mut sections = Sections::default();
        sections
            .push(
                &header,
                (0x1000, (CAP + 1) as u64, (CAP + 1) as u64, 0, 3),
                true,
            )
            .unwrap();
        assert!(sections.is_empty());
        assert!(sections.parts.is_empty());
        for _ in 0..64 {
            sections
                .push(
                    &header,
                    (0x1000, (CAP + 1) as u64, (CAP + 1) as u64, 0, 3),
                    true,
                )
                .unwrap();
        }
        assert!(
            sections.is_empty() && sections.data_overflow && sections.ordinary_data.len() == 64
        );
        let (mut overflow, r) = loaded();
        overflow.objc_sections.data_overflow = true;
        rejects(overflow, r);
        let (object, _) = loaded();
        let (list, _) = object
            .objc_sections
            .section("__objc_protolist", 0)
            .unwrap()
            .unwrap();
        let p = word(&object, list);
        let (classlist, _) = object
            .objc_sections
            .section("__objc_classlist", 0)
            .unwrap()
            .unwrap();
        let class = word(&object, classlist);
        let (mut bad, r) = loaded();
        overwrite(&mut bad, list, &class.to_le_bytes());
        let data = &mut bad.objc_sections.ordinary_data[0];
        let base = data.segment.0;
        let file_offset = data.segment.3;
        let size = (p + 96) - class;
        data.header[32..40].copy_from_slice(&class.to_le_bytes());
        data.header[40..48].copy_from_slice(&size.to_le_bytes());
        data.header[48..52].copy_from_slice(&((file_offset + class - base) as u32).to_le_bytes());
        rejects(bad, r);
        let (mut bad, r) = loaded();
        bad.objc_sections.ordinary_data[0].header[40..48].copy_from_slice(&8u64.to_le_bytes());
        rejects(bad, r);
        let (mut bad, r) = loaded();
        bad.objc_sections.ordinary_data[0].header[64..68].copy_from_slice(&1u32.to_le_bytes());
        rejects(bad, r);
    }
}
