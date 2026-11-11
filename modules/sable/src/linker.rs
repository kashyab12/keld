//! Restricted eager Mach-O linking, implemented from published format contracts.
//! Only explicitly supplied first-party dylibs are eligible for name resolution.
use crate::{
    Result,
    macho::{Image, Object},
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

const LIMIT: usize = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Symbol {
    pub address: u64,
    pub tls: bool,
}
impl Symbol {
    pub fn addend(self, addend: i64) -> Result<Self> {
        if self.tls && addend != 0 {
            return Err("TLS descriptor addends unsupported".into());
        }
        Ok(Self {
            address: self
                .address
                .checked_add_signed(addend)
                .ok_or("bind addend overflow")?,
            ..self
        })
    }
}

fn byte(data: &[u8], cursor: &mut usize) -> Result<u8> {
    let b = *data.get(*cursor).ok_or("truncated dyld stream")?;
    *cursor += 1;
    Ok(b)
}
pub(crate) fn uleb(data: &[u8], cursor: &mut usize) -> Result<u64> {
    let mut value = 0u128;
    for n in 0..10 {
        let b = byte(data, cursor)?;
        value |= u128::from(b & 127) << (n * 7);
        if b & 128 == 0 {
            return u64::try_from(value).map_err(|_| "ULEB128 overflow".into());
        }
    }
    Err("unterminated/overflowing ULEB128".into())
}
fn sleb(data: &[u8], cursor: &mut usize) -> Result<i64> {
    let mut value = 0i128;
    for n in 0..10 {
        let b = byte(data, cursor)?;
        value |= i128::from(b & 127) << (n * 7);
        if b & 128 == 0 {
            if b & 64 != 0 {
                value |= (!0i128) << ((n + 1) * 7);
            }
            return i64::try_from(value).map_err(|_| "SLEB128 overflow".into());
        }
    }
    Err("unterminated/overflowing SLEB128".into())
}
fn cstring(data: &[u8], cursor: &mut usize) -> Result<String> {
    let start = *cursor;
    while byte(data, cursor)? != 0 {
        if *cursor - start > 4096 {
            return Err("dyld symbol exceeds 4096 bytes".into());
        }
    }
    String::from_utf8(data[start..*cursor - 1].to_vec())
        .map_err(|_| "non-UTF8 dyld symbol unsupported".into())
}

fn exports(data: &[u8], base: u64) -> Result<BTreeMap<String, Symbol>> {
    fn walk(
        data: &[u8],
        at: usize,
        prefix: String,
        base: u64,
        path: &mut BTreeSet<usize>,
        out: &mut BTreeMap<String, Symbol>,
        visits: &mut usize,
    ) -> Result<()> {
        *visits += 1;
        if *visits > 4096 || path.len() >= 64 || !path.insert(at) {
            return Err("cyclic/excessive export trie".into());
        }
        let mut cursor = at;
        let terminal =
            usize::try_from(uleb(data, &mut cursor)?).map_err(|_| "export terminal overflow")?;
        let end = cursor
            .checked_add(terminal)
            .ok_or("export terminal overflow")?;
        if end >= data.len() {
            return Err("export terminal exceeds trie".into());
        }
        if terminal > 0 {
            let mut t = cursor;
            let flags = uleb(&data[..end], &mut t)?;
            if flags > 2 {
                return Err(format!("unsupported export flags {flags:#x}"));
            }
            let value = uleb(&data[..end], &mut t)?;
            if t != end || prefix.is_empty() {
                return Err("invalid export terminal".into());
            }
            let address = if flags == 2 {
                value
            } else {
                base.checked_add(value).ok_or("export address overflow")?
            };
            if out
                .insert(
                    prefix.clone(),
                    Symbol {
                        address,
                        tls: flags == 1,
                    },
                )
                .is_some()
            {
                return Err("duplicate export symbol".into());
            }
        }
        cursor = end;
        let children = byte(data, &mut cursor)?;
        for _ in 0..children {
            let edge = cstring(data, &mut cursor)?;
            if edge.is_empty() || prefix.len() + edge.len() > 4096 {
                return Err("invalid export edge".into());
            }
            let child =
                usize::try_from(uleb(data, &mut cursor)?).map_err(|_| "export child overflow")?;
            walk(
                data,
                child,
                format!("{prefix}{edge}"),
                base,
                path,
                out,
                visits,
            )?;
        }
        path.remove(&at);
        Ok(())
    }
    let mut out = BTreeMap::new();
    if !data.is_empty() {
        walk(
            data,
            0,
            String::new(),
            base,
            &mut BTreeSet::new(),
            &mut out,
            &mut 0,
        )?;
    }
    Ok(out)
}

#[derive(Default)]
struct Position {
    segment: Option<usize>,
    offset: u64,
}
impl Position {
    fn select(&mut self, object: &Object, index: usize, offset: u64) -> Result<()> {
        let mapped = object
            .segment_indices
            .get(index)
            .copied()
            .flatten()
            .ok_or("invalid/inaccessible dyld segment index")?;
        if offset > object.image.segments[mapped].bytes.len() as u64 {
            return Err("dyld offset outside segment".into());
        }
        self.segment = Some(mapped);
        self.offset = offset;
        Ok(())
    }
    fn advance(&mut self, amount: u64) -> Result<()> {
        self.offset = self
            .offset
            .checked_add(amount)
            .ok_or("dyld offset overflow")?;
        Ok(())
    }
    fn bind_advance(&mut self, object: &Object, amount: u64) -> Result<()> {
        // Legacy bind deltas are 64-bit unsigned cursor arithmetic. LLVM can
        // encode a backwards step as a large ULEB, including after DO_BIND.
        // The resulting cursor must still belong to the selected segment.
        let index = self.segment.ok_or("dyld segment not selected")?;
        let next = self.offset.wrapping_add(amount);
        if next > object.image.segments[index].bytes.len() as u64 {
            return Err("dyld bind cursor outside selected segment".into());
        }
        self.offset = next;
        Ok(())
    }
    fn slot(&self, object: &Object) -> Result<(usize, usize)> {
        let index = self.segment.ok_or("dyld segment not selected")?;
        let region = &object.image.segments[index];
        let offset = usize::try_from(self.offset).map_err(|_| "dyld slot overflow")?;
        if region.protection & 4 != 0
            || offset.checked_add(8).ok_or("dyld slot overflow")? > region.bytes.len()
        {
            return Err("dyld pointer slot outside non-executable storage".into());
        }
        Ok((index, offset))
    }
}
fn bounded_count(n: u64) -> Result<usize> {
    let n = usize::try_from(n).map_err(|_| "dyld count overflow")?;
    if n > LIMIT {
        return Err("excessive dyld operation count".into());
    }
    Ok(n)
}

fn rebase(object: &mut Object, slide: u64) -> Result<()> {
    let data = object.streams[0].clone();
    if data.is_empty() {
        return Ok(());
    }
    let mut cursor = 0;
    let mut position = Position::default();
    let mut pointer = false;
    let mut operations = 0;
    while cursor < data.len() {
        let b = byte(&data, &mut cursor)?;
        let immediate = b & 15;
        let (count, skip) = match b & 0xf0 {
            0x00 => {
                if b != 0 || data[cursor..].iter().any(|v| *v != 0) {
                    return Err("invalid rebase DONE/trailing data".into());
                }
                return Ok(());
            }
            0x10 => {
                if immediate != 1 {
                    return Err("only pointer rebasing supported".into());
                }
                pointer = true;
                continue;
            }
            0x20 => {
                position.select(object, immediate as usize, uleb(&data, &mut cursor)?)?;
                continue;
            }
            0x30 => {
                position.advance(uleb(&data, &mut cursor)?)?;
                continue;
            }
            0x40 => {
                position.advance(u64::from(immediate) * 8)?;
                continue;
            }
            0x50 => (immediate as usize, 0),
            0x60 => (bounded_count(uleb(&data, &mut cursor)?)?, 0),
            0x70 => (1, uleb(&data, &mut cursor)?),
            0x80 => (
                bounded_count(uleb(&data, &mut cursor)?)?,
                uleb(&data, &mut cursor)?,
            ),
            _ => return Err("unsupported rebase opcode".into()),
        };
        if !pointer {
            return Err("rebase type not selected".into());
        }
        operations += count;
        if operations > LIMIT {
            return Err("excessive rebasing".into());
        }
        for _ in 0..count {
            let (index, offset) = position.slot(object)?;
            let bytes = std::sync::Arc::make_mut(&mut object.image.segments[index].bytes);
            let original = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
            let value = original
                .checked_add(slide)
                .ok_or("rebased pointer overflow")?;
            bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
            position.advance(8u64.checked_add(skip).ok_or("rebase stride overflow")?)?;
        }
    }
    Err("rebase stream missing DONE".into())
}

fn resolve_symbol(
    object: &Object,
    object_index: usize,
    ordinal: i64,
    name: &str,
    all_exports: &[BTreeMap<String, Symbol>],
    ids: &HashMap<String, usize>,
) -> Result<Symbol> {
    let source = match ordinal {
        0 => object_index,
        -1 => 0,
        n if n > 0 => {
            let dependency = object
                .dependencies
                .get((n - 1) as usize)
                .ok_or("bind ordinal outside dependency list")?;
            *ids.get(&dependency.0)
                .ok_or("bind dependency not supplied")?
        }
        _ => return Err("flat/weak namespace binding unsupported".into()),
    };
    all_exports[source]
        .get(name)
        .copied()
        .ok_or_else(|| format!("unresolved import {name} (ordinal {ordinal})"))
}

fn bind(
    object: &mut Object,
    stream: usize,
    object_index: usize,
    all_exports: &[BTreeMap<String, Symbol>],
    ids: &HashMap<String, usize>,
) -> Result<()> {
    let data = object.streams[stream].clone();
    if data.is_empty() {
        return Ok(());
    }
    let lazy = stream == 3;
    let mut cursor = 0;
    let mut position = Position::default();
    let mut ordinal = 0i64;
    let mut symbol = None;
    let mut addend = 0i64;
    let mut operations = 0;
    let mut finished = false;
    while cursor < data.len() {
        let b = byte(&data, &mut cursor)?;
        let immediate = b & 15;
        if b != 0 {
            finished = false;
        }
        let (count, skip) = match b & 0xf0 {
            0x00 => {
                if b != 0 {
                    return Err("invalid bind DONE".into());
                }
                if !lazy {
                    if data[cursor..].iter().any(|v| *v != 0) {
                        return Err("nonzero bytes after bind DONE".into());
                    }
                    return Ok(());
                }
                finished = true;
                position = Position::default();
                ordinal = 0;
                symbol = None;
                addend = 0;
                continue;
            }
            0x10 => {
                ordinal = i64::from(immediate);
                continue;
            }
            0x20 => {
                ordinal = i64::try_from(uleb(&data, &mut cursor)?)
                    .map_err(|_| "bind ordinal overflow")?;
                continue;
            }
            0x30 => {
                ordinal = if immediate == 0 {
                    0
                } else {
                    i64::from(immediate as i8 | !15)
                };
                continue;
            }
            0x40 => {
                if immediate != 0 {
                    return Err("weak/special symbol binding unsupported".into());
                }
                symbol = Some(cstring(&data, &mut cursor)?);
                continue;
            }
            0x50 => {
                if immediate != 1 {
                    return Err("only pointer binding supported".into());
                }
                continue;
            }
            0x60 => {
                addend = sleb(&data, &mut cursor)?;
                continue;
            }
            0x70 => {
                position.select(object, immediate as usize, uleb(&data, &mut cursor)?)?;
                continue;
            }
            0x80 => {
                position.bind_advance(object, uleb(&data, &mut cursor)?)?;
                continue;
            }
            0x90 => (1, 0),
            0xa0 => (1, uleb(&data, &mut cursor)?),
            0xb0 => (1, u64::from(immediate) * 8),
            0xc0 => (
                bounded_count(uleb(&data, &mut cursor)?)?,
                uleb(&data, &mut cursor)?,
            ),
            _ => return Err("unsupported/threaded bind opcode".into()),
        };
        let name = symbol.as_ref().ok_or("bind symbol not selected")?;
        let value = resolve_symbol(object, object_index, ordinal, name, all_exports, ids)?
            .addend(addend)?
            .address;
        operations += count;
        if operations > LIMIT {
            return Err("excessive binding".into());
        }
        for _ in 0..count {
            let (index, offset) = position.slot(object)?;
            std::sync::Arc::make_mut(&mut object.image.segments[index].bytes)[offset..offset + 8]
                .copy_from_slice(&value.to_le_bytes());
            position.bind_advance(object, 8)?;
            position.bind_advance(object, skip)?;
        }
    }
    if finished {
        Ok(())
    } else {
        Err("bind stream missing DONE".into())
    }
}

pub fn link(main: &[u8], libraries: &[&[u8]]) -> Result<Image> {
    link_entry(main, libraries, false)
}
pub fn link_main(main: &[u8], libraries: &[&[u8]]) -> Result<Image> {
    link_entry(main, libraries, true)
}
fn link_entry(main: &[u8], libraries: &[&[u8]], main_entry: bool) -> Result<Image> {
    if libraries.len() > 31 {
        return Err("at most 31 explicitly supplied libraries".into());
    }
    let root = Object::parse(main, true)?;
    if root.kind != 2 {
        return Err("link root must be MH_EXECUTE".into());
    }
    let mut objects = vec![root];
    let mut mapped = objects[0]
        .image
        .segments
        .iter()
        .map(|s| s.bytes.len())
        .sum::<usize>();
    let mut ids = HashMap::new();
    for bytes in libraries {
        let object = Object::parse(bytes, true)?;
        mapped = mapped
            .checked_add(
                object
                    .image
                    .segments
                    .iter()
                    .map(|s| s.bytes.len())
                    .sum::<usize>(),
            )
            .ok_or("loaded memory size overflow")?;
        if mapped > 64 * 1024 * 1024 {
            return Err("explicitly supplied images exceed 64 MiB".into());
        }
        if object.kind != 6 {
            return Err("supplied library must be MH_DYLIB".into());
        }
        let name = object.id.clone().ok_or("dylib missing LC_ID_DYLIB")?;
        if ids.insert(name, objects.len()).is_some() {
            return Err("duplicate dylib identity".into());
        }
        objects.push(object);
    }
    let mut order = vec![0];
    let mut visited = BTreeSet::from([0]);
    let mut cursor = 0;
    while cursor < order.len() {
        for (name, minimum) in &objects[order[cursor]].dependencies {
            let index = *ids
                .get(name)
                .ok_or_else(|| format!("required dylib not supplied: {name}"))?;
            if objects[index].current_version < *minimum {
                return Err(format!("incompatible dylib version: {name}"));
            }
            if visited.insert(index) {
                order.push(index);
            }
        }
        cursor += 1;
    }
    let mut all_exports = vec![BTreeMap::new(); objects.len()];
    let mut slides = vec![0; objects.len()];
    for (placement, index) in order.iter().copied().enumerate() {
        let object = &mut objects[index];
        let slide = if index == 0 {
            0
        } else {
            (0x2_0000_0000u64 + (placement as u64 - 1) * 0x1000_0000)
                .checked_sub(object.header_base)
                .ok_or("unsupported dylib preferred address")?
        };
        slides[index] = slide;
        for segment in &mut object.image.segments {
            segment.address = segment
                .address
                .checked_add(slide)
                .ok_or("slid segment overflow")?;
            segment
                .address
                .checked_add(segment.bytes.len() as u64)
                .ok_or("slid segment end overflow")?;
        }
        object.image.entry = object
            .image
            .entry
            .checked_add(slide)
            .ok_or("slid entry overflow")?;
        object.image.text.0 = object
            .image
            .text
            .0
            .checked_add(slide)
            .ok_or("slid text overflow")?;
        for range in &mut object.image.extra_text {
            range.0 = range.0.checked_add(slide).ok_or("slid text overflow")?;
        }
        all_exports[index] = exports(
            &object.streams[4],
            object
                .header_base
                .checked_add(slide)
                .ok_or("slid header overflow")?,
        )?;
        for symbol in all_exports[index].values() {
            if symbol.tls
                && !object
                    .tls_sections
                    .contains_descriptor(symbol.address, slide)
            {
                return Err("TLS export does not name an owning-image descriptor".into());
            }
        }
        if !object.streams[2].is_empty() {
            return Err("weak bind stream unsupported".into());
        }
        rebase(object, slide)?;
    }
    for index in order.iter().copied() {
        bind(&mut objects[index], 1, index, &all_exports, &ids)?;
        bind(&mut objects[index], 3, index, &all_exports, &ids)?;
        let object = &objects[index];
        let patches = crate::chained::patches(object, slides[index], |ordinal, name| {
            resolve_symbol(object, index, ordinal, name, &all_exports, &ids)
        })?;
        let object = &mut objects[index];
        for (segment, offset, value) in patches {
            std::sync::Arc::make_mut(&mut object.image.segments[segment].bytes)[offset..offset + 8]
                .copy_from_slice(&value.to_le_bytes());
        }
        for segment in &object.readonly {
            object.image.segments[*segment].protection &= !2;
        }
    }
    let mut initialization = Vec::new();
    if order.iter().any(|&i| !objects[i].init_sections.is_empty()) {
        fn visit(
            index: usize,
            objects: &[Object],
            ids: &HashMap<String, usize>,
            state: &mut [u8],
            out: &mut Vec<usize>,
        ) -> Result<()> {
            if state[index] == 1 {
                return Err("cyclic initializer dependencies unsupported".into());
            }
            if state[index] == 2 {
                return Ok(());
            }
            state[index] = 1;
            for (name, _) in &objects[index].dependencies {
                visit(ids[name], objects, ids, state, out)?;
            }
            state[index] = 2;
            out.push(index);
            Ok(())
        }
        let mut sorted = Vec::new();
        visit(0, &objects, &ids, &mut vec![0; objects.len()], &mut sorted)?;
        for index in sorted {
            let object = &objects[index];
            let base = object
                .header_base
                .checked_add(slides[index])
                .ok_or("initializer base overflow")?;
            for section in &object.init_sections {
                let address = section
                    .address
                    .checked_add(slides[index])
                    .ok_or("initializer array overflow")?;
                let bytes = object.image.read(address, section.size, 1)?;
                for at in (0..section.size).step_by(if section.offsets { 4 } else { 8 }) {
                    let target = if section.offsets {
                        base.checked_add(u64::from(crate::macho::u32_at(bytes, at)?))
                            .ok_or("initializer target overflow")?
                    } else {
                        crate::macho::u64_at(bytes, at)?
                    };
                    if !object.image.contains_pc(target) {
                        return Err("initializer target outside owning image code".into());
                    }
                    if initialization.len() == 1024 {
                        return Err("at most 1024 initializers".into());
                    }
                    initialization.push(target);
                }
            }
        }
    }
    let objc_runtime = if order.iter().any(|&i| !objects[i].objc_sections.is_empty()) {
        let index = *ids
            .get("/usr/lib/libobjc.A.dylib")
            .ok_or("ObjC requires explicit libobjc runtime")?;
        if !order.contains(&index) {
            return Err("ObjC runtime not loaded".into());
        }
        let symbol = |name: &str| -> Result<u64> {
            let s = all_exports[index]
                .get(name)
                .ok_or_else(|| format!("missing ObjC runtime export: {name}"))?;
            if s.tls {
                return Err("ObjC runtime export cannot be TLS".into());
            }
            // Runtime entry points/data must belong to this explicit library.
            objects[index].image.read(s.address, 8, 1)?;
            Ok(s.address)
        };
        let empty_cache = symbol("__objc_empty_cache")?;
        objects[index].image.read(empty_cache, 8, 3)?;
        let protocols = if [
            "_objc_getProtocol",
            "_protocol_getName",
            "_class_conformsToProtocol",
            "_sable_protocol_nil",
        ]
        .iter()
        .any(|name| all_exports[index].contains_key(*name))
        {
            Some(crate::objc::protocol::Runtime {
                get: symbol("_objc_getProtocol")?,
                name: symbol("_protocol_getName")?,
                conforms: symbol("_class_conformsToProtocol")?,
                nil_name: symbol("_sable_protocol_nil")?,
            })
        } else {
            None
        };
        let constants = if order
            .iter()
            .any(|&i| objects[i].objc_sections.has_constants())
        {
            let provider = *ids
                .get(crate::objc::constants::CORE_FOUNDATION)
                .ok_or("constant strings require CoreFoundation provider")?;
            let foundation = *ids
                .get(crate::objc::constants::FOUNDATION)
                .ok_or("constant strings require Foundation provider")?;
            if !order.contains(&provider) || !order.contains(&foundation) {
                return Err("constant framework provider not loaded".into());
            }
            let alias = all_exports[provider]
                .get("___CFConstantStringClassReference")
                .ok_or("missing constant-string class alias")?;
            if alias.tls {
                return Err("constant-string class alias cannot be TLS".into());
            }
            Some(crate::objc::constants::Provider {
                index: provider,
                foundation,
                class: alias.address,
            })
        } else {
            None
        };
        let ownership = if crate::objc::ownership::NAMES
            .iter()
            .any(|n| all_exports[index].contains_key(*n))
        {
            if all_exports[index].contains_key("_objc_autoreleaseReturnValue") {
                return Err("optimized autoreleaseReturnValue producer unsupported".into());
            }
            let mut entries = [0; crate::objc::ownership::BRIDGES];
            for (i, name) in crate::objc::ownership::NAMES.iter().enumerate() {
                entries[i] = symbol(name)?;
            }
            // The extra register bridges are one all-or-nothing table.
            let names: Vec<_> = (crate::objc::ownership::NAMES.len()..entries.len())
                .map(crate::objc::ownership::name)
                .collect();
            let extended = names.iter().any(|n| all_exports[index].contains_key(n));
            if extended {
                for (offset, name) in names.iter().enumerate() {
                    entries[crate::objc::ownership::NAMES.len() + offset] = symbol(name)?;
                }
            }
            Some(crate::objc::ownership::Runtime { entries, extended })
        } else {
            None
        };
        Some(crate::objc::Runtime {
            get_class: symbol("_objc_getClass")?,
            msg_send: symbol("_objc_msgSend")?,
            empty_cache,
            instances: if ["_class_createInstance", "_object_dispose"]
                .iter()
                .any(|name| all_exports[index].contains_key(*name))
            {
                Some(crate::objc::instances::Runtime {
                    create: symbol("_class_createInstance")?,
                    dispose: symbol("_object_dispose")?,
                })
            } else {
                None
            },
            super_send: all_exports[index]
                .contains_key("_objc_msgSendSuper2")
                .then(|| symbol("_objc_msgSendSuper2"))
                .transpose()?,
            protocols,
            constants,
            ownership,
            ui: {
                const UIKIT: &str = "/System/Library/Frameworks/UIKit.framework/UIKit";
                const NAMES: [&str; 3] = [
                    "_sable_ui_launch_info",
                    "_sable_ui_present",
                    "_sable_ui_wait_event",
                ];
                match ids.get(UIKIT).copied().filter(|i| order.contains(i)) {
                    Some(ui) if NAMES.iter().any(|n| all_exports[ui].contains_key(*n)) => {
                        let mut addresses = [0u64; 3];
                        for (slot, name) in addresses.iter_mut().zip(NAMES) {
                            let s = all_exports[ui]
                                .get(name)
                                .ok_or_else(|| format!("missing UI service export: {name}"))?;
                            if s.tls {
                                return Err("UI service export cannot be TLS".into());
                            }
                            objects[ui].image.read(s.address, 8, 1)?;
                            *slot = s.address;
                        }
                        Some(crate::ui::Runtime {
                            launch_info: addresses[0],
                            present: addresses[1],
                            wait_event: addresses[2],
                        })
                    }
                    _ => None,
                }
            },
            reflection: if [
                "_class_respondsToSelector",
                "_class_getName",
                "_sable_class_cxx_destruct",
            ]
            .iter()
            .any(|name| all_exports[index].contains_key(*name))
            {
                Some(crate::objc::Reflection {
                    responds: symbol("_class_respondsToSelector")?,
                    name: symbol("_class_getName")?,
                    destruct: symbol("_sable_class_cxx_destruct")?,
                })
            } else {
                None
            },
        })
    } else {
        None
    };
    let objc = crate::objc::register(&mut objects, &order, &slides, objc_runtime)?;
    let mut image = objects[0].image.clone();
    image.objc = objc;
    for &index in &order {
        let object = &objects[index];
        if !object.tls_sections.is_empty() {
            let ordinal = object
                .dependencies
                .iter()
                .position(|(name, _)| name == "/usr/lib/libSystem.B.dylib")
                .ok_or("TLS requires explicit LibSystem bootstrap dependency")?
                as i64
                + 1;
            let bootstrap = resolve_symbol(
                object,
                index,
                ordinal,
                "__tlv_bootstrap",
                &all_exports,
                &ids,
            )?;
            if bootstrap.tls {
                return Err("TLS bootstrap must be a regular code export".into());
            }
            image.tls.push(
                object
                    .tls_sections
                    .finish(object, slides[index], bootstrap.address)?,
            );
        }
    }
    if !initialization.is_empty() || main_entry {
        image.startup = Some(crate::macho::Startup {
            header: objects[0].header_base,
            initializers: initialization,
        });
    }
    let tls_targets = image
        .tls
        .iter()
        .flat_map(|b| b.descriptors.iter().map(|d| d.address))
        .collect();
    for &index in &order {
        objects[index].tls_sections.validate_pointers(
            &objects[index].image,
            slides[index],
            &tls_targets,
        )?;
    }
    for index in order.into_iter().skip(1) {
        image.extra_text.extend(objects[index].image.code_ranges());
        image.segments.append(&mut objects[index].image.segments);
    }
    let mut ranges: Vec<_> = image.code_ranges().collect();
    ranges.sort_unstable();
    image.segments.sort_by_key(|s| s.address);
    crate::tls::layout(&image)?;
    if let Some(plan) = &image.objc {
        plan.validate(&image)?;
    }
    if image
        .segments
        .windows(2)
        .any(|s| s[0].address + s[0].bytes.len() as u64 > s[1].address)
        || image.segments.iter().map(|s| s.bytes.len()).sum::<usize>() > 64 * 1024 * 1024
    {
        return Err("overlapping/excessive linked segments".into());
    }
    if ranges.windows(2).any(|r| r[0].0 + r[0].1 > r[1].0)
        || ranges.iter().map(|r| r.1).sum::<u64>() > 1024 * 1024
    {
        return Err("overlapping/excessive linked code".into());
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tls_export_kind_and_addend_contract() {
        let trie = [0, 1, b'x', 0, 5, 3, 1, 0x80, 0x20, 0];
        let symbol = exports(&trie, 0x1000).unwrap()["x"];
        assert_eq!(
            symbol,
            Symbol {
                address: 0x2000,
                tls: true
            }
        );
        assert_eq!(symbol.addend(0).unwrap(), symbol);
        for addend in [1, -1, i64::MAX, i64::MIN] {
            assert!(symbol.addend(addend).is_err());
        }
        for flags in [3, 4, 8, 16, 32] {
            let mut invalid = trie;
            invalid[6] = flags;
            assert!(exports(&invalid, 0x1000).is_err());
        }
        assert_eq!(
            Symbol {
                address: 0x1000,
                tls: false
            }
            .addend(-1)
            .unwrap()
            .address,
            0xfff
        );
    }
    #[test]
    fn bounded_integer_and_trie_parsing() {
        for bytes in [&[0x80][..], &[0xff; 10][..], &[0x80; 11][..]] {
            assert!(uleb(bytes, &mut 0).is_err());
        }
        assert_eq!(sleb(&[0x7f], &mut 0).unwrap(), -1);
        assert_eq!(sleb(&[0x80, 0x7f], &mut 0).unwrap(), -128);
        assert!(sleb(&[0xff; 10], &mut 0).is_err());
        assert!(exports(&[0, 1, b'a', 0, 0], 0).is_err());
        assert!(exports(&[255], 0).is_err());
    }

    #[test]
    fn legacy_bind_backwards_deltas_keep_every_cursor_inside_selected_storage() {
        let raw = include_bytes!("../fixtures/bin/dynamic/dynamic-8.macho");
        fn encoded(mut v: u64) -> Vec<u8> {
            let mut out = Vec::new();
            loop {
                let byte = (v & 127) as u8;
                v >>= 7;
                out.push(byte | if v != 0 { 128 } else { 0 });
                if v == 0 {
                    return out;
                }
            }
        }
        let mut step = vec![0x90, 0x80];
        step.extend(encoded(u64::MAX - 15));
        step.extend([0x90, 0]);
        let mut combined = vec![0xa0];
        combined.extend(encoded(u64::MAX - 15));
        combined.extend([0x90, 0]);
        let mut repeated = vec![0xc0, 2];
        repeated.extend(encoded(u64::MAX - 15));
        repeated.push(0);
        for tail in [step, combined, repeated] {
            let mut o = Object::parse(raw, true).unwrap();
            let rw = o
                .image
                .segments
                .iter()
                .position(|s| s.protection == 3)
                .unwrap();
            let disk_index = o
                .segment_indices
                .iter()
                .position(|&i| i == Some(rw))
                .unwrap();
            let mut stream = vec![0x11, 0x40];
            stream.extend(b"_sable_mix\0");
            stream.extend([0x51, 0x70 | disk_index as u8, 16]);
            stream.extend(tail);
            o.streams[1] = stream;
            let maps = vec![
                BTreeMap::new(),
                BTreeMap::from([(
                    "_sable_mix".to_string(),
                    Symbol {
                        address: 0x200000388,
                        tls: false,
                    },
                )]),
            ];
            let ids = HashMap::from([("@loader_path/libsable_math.dylib".to_string(), 1)]);
            bind(&mut o, 1, 0, &maps, &ids).unwrap();
            for at in [8, 16] {
                assert_eq!(
                    &o.image.segments[rw].bytes[at..at + 8],
                    &0x200000388u64.to_le_bytes()
                );
            }
            let mut position = Position {
                segment: Some(rw),
                offset: 16,
            };
            assert!(position.bind_advance(&o, u64::MAX - 31).is_err());
            assert_eq!(position.offset, 16);
            assert!(
                position
                    .bind_advance(&o, o.image.segments[rw].bytes.len() as u64)
                    .is_err()
            );
            assert!(Position::default().bind_advance(&o, 0).is_err());
        }
    }

    #[test]
    fn malformed_binding_streams_never_patch_executable_storage() {
        let raw = include_bytes!("../fixtures/bin/dynamic/dynamic-8.macho");
        let symbol = b"_sable_mix\0";
        let mut code_patch = vec![0x11, 0x40];
        code_patch.extend(symbol);
        code_patch.extend([0x71, 0, 0x90, 0]);
        let mut huge_offset = vec![0x72];
        huge_offset.extend([0xff; 10]);
        for stream in [
            vec![0x90, 0],
            vec![0x53, 0],
            vec![0xd0, 0],
            vec![0x70, 0, 0],
            huge_offset,
            code_patch,
        ] {
            let mut object = Object::parse(raw, true).unwrap();
            object.streams[1] = stream;
            let snapshot = object.image.segments[0].bytes.clone();
            let maps = vec![
                BTreeMap::new(),
                BTreeMap::from([(
                    "_sable_mix".to_string(),
                    Symbol {
                        address: 0x200000388,
                        tls: false,
                    },
                )]),
            ];
            let ids = HashMap::from([("@loader_path/libsable_math.dylib".to_string(), 1)]);
            assert!(bind(&mut object, 1, 0, &maps, &ids).is_err());
            assert_eq!(object.image.segments[0].bytes, snapshot);
        }
    }
}
