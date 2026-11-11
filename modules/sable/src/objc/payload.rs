//! Original bounded Q ivar layout; offsets are metadata, never inferred from code.
use super::*;
pub(super) const MAX_SIZE: u32 = 64;
#[derive(Debug)]
pub(super) struct Layout {
    pub start: u32,
    pub size: u32,
    pub names: Vec<String>,
}
pub(super) fn parse(
    object: &Object,
    slide: u64,
    ro: &[u8],
    root: bool,
    used_cells: &mut BTreeSet<u64>,
) -> Result<Layout> {
    let start = u32_at(ro, 4)?;
    let size = u32_at(ro, 8)?;
    if (root && start != 0)
        || (!root && (start < 8 || !start.is_multiple_of(8)))
        || size < 8
        || size < start
        || size > MAX_SIZE
        || !size.is_multiple_of(8)
    {
        return Err("unsupported ObjC payload start/size".into());
    }
    let list = u64_at(ro, 48)?;
    let mut names = BTreeSet::new();
    let mut offsets = Vec::new();
    if list != 0 {
        let head = record(object, slide, "__objc_const", list, 8)?;
        let count = u32_at(head, 4)? as usize;
        if u32_at(head, 0)? != 32 || count > 8 {
            return Err("unsupported ObjC ivar list stride/count".into());
        }
        let rows = record(object, slide, "__objc_const", list, 8 + count * 32)?;
        let (base, len) = object
            .objc_sections
            .section("__objc_ivar", slide)?
            .ok_or("missing owning ObjC offset cells")?;
        if !base.is_multiple_of(4) || len % 4 != 0 {
            return Err("invalid ObjC offset-cell section".into());
        }
        for n in 0..count {
            let row = &rows[8 + n * 32..8 + (n + 1) * 32];
            let cell = u64_at(row, 0)?;
            object
                .objc_sections
                .contains("__objc_ivar", slide, cell, 4)?;
            if !cell.is_multiple_of(4)
                || !(cell - base).is_multiple_of(4)
                || !used_cells.insert(cell)
            {
                return Err("unaligned/duplicate ObjC ivar offset cell".into());
            }
            let offset = u32_at(object.image.read(cell, 4, 1)?, 0)?;
            let name = owned_string(object, slide, "__objc_methname", u64_at(row, 8)?)?;
            let ty = owned_string(object, slide, "__objc_methtype", u64_at(row, 16)?)?;
            if !names.insert(name.clone()) || u32_at(row, 24)? != 3 || u32_at(row, 28)? != 8 {
                return Err("unsupported/duplicate ObjC ivar name/alignment/size".into());
            }
            if ty == "#" && root && offset == 0 && name == "isa" {
                continue;
            }
            if ty != "Q"
                || name == "isa"
                || !offset.is_multiple_of(8)
                || offset < if root { 8 } else { start }
                || offset.checked_add(8).is_none_or(|end| end > size)
            {
                return Err("unsupported ObjC payload ivar type/range".into());
            }
            offsets.push(offset);
        }
    }
    offsets.sort_unstable();
    let mut cursor = if root { 8 } else { start };
    for offset in offsets {
        if offset != cursor {
            return Err("overlapping or noncontiguous ObjC payload ivars".into());
        }
        cursor += 8;
    }
    if cursor != size {
        return Err("ObjC size lacks contiguous payload coverage".into());
    }
    Ok(Layout {
        start,
        size,
        names: names.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn loaded(profile: u8) -> (Object, Runtime) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("fixtures/bin/payload/absolute/chained/{profile}/0"));
        let raw = std::fs::read(root.join("payload-0.macho")).unwrap();
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
    fn word(o: &Object, a: u64) -> u64 {
        u64_at(o.image.read(a, 8, 1).unwrap(), 0).unwrap()
    }
    fn patch_bytes(o: &mut Object, a: u64, bytes: &[u8]) {
        for s in &mut o.image.segments {
            if let Some(n) = a.checked_sub(s.address).map(|n| n as usize)
                && n.checked_add(bytes.len())
                    .is_some_and(|end| end <= s.bytes.len())
            {
                std::sync::Arc::make_mut(&mut s.bytes)[n..n + bytes.len()].copy_from_slice(bytes);
                return;
            }
        }
        panic!("bad patch");
    }
    fn ids(o: &Object) -> (u64, u64, u64) {
        let (list, len) = o
            .objc_sections
            .section("__objc_classlist", 0)
            .unwrap()
            .unwrap();
        assert_eq!(len, 16);
        let rows = [word(o, list), word(o, list + 8)];
        let child = *rows.iter().find(|&&c| word(o, c + 8) != 0).unwrap();
        (word(o, child + 32), word(o, word(o, child + 8) + 32), list)
    }
    fn reject(o: Object, r: Runtime) {
        assert!(register(&mut [o], &[0], &[0], Some(r)).is_err());
    }
    #[test]
    fn validated_class_sizes_drive_requested_span_without_allocator_rounding() {
        for (profile, rs, cs) in [(0, 16, 24), (1, 8, 16), (2, 64, 64), (3, 16, 64)] {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("fixtures/bin/payload/absolute/chained/{profile}/0"));
            let raw = std::fs::read(root.join("payload-0.macho")).unwrap();
            let libs = ["libsable_objc.dylib", "libsable_system.dylib"]
                .map(|n| std::fs::read(root.join(n)).unwrap());
            let image =
                Image::link(&raw, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
            let plan = image.objc.as_ref().unwrap();
            let runtime = plan.runtime.instances.unwrap();
            let mut memory = Memory::new(&image).unwrap();
            for (n, (parent, size)) in [(true, rs), (false, cs)].into_iter().enumerate() {
                let class = plan
                    .classes
                    .iter()
                    .find(|c| c.superclass.is_none() == parent)
                    .unwrap();
                assert_eq!(class.size, size);
                let address = plan
                    .trap(
                        &mut memory,
                        runtime.create,
                        instances::CREATE,
                        class.address,
                        64,
                        0,
                    )
                    .unwrap();
                assert_eq!(
                    memory.objects.as_ref().unwrap().rows[n].extent,
                    size as usize + 64
                );
                assert_eq!(memory.read(address + size as u64, 64).unwrap(), [0; 64]);
                assert_eq!(
                    plan.trap(
                        &mut memory,
                        runtime.dispose,
                        instances::DISPOSE,
                        address,
                        0,
                        0
                    )
                    .unwrap(),
                    0
                );
            }
        }
    }
    #[test]
    fn exact_owner_cells_rows_types_ranges_and_unique_names_reject_malformed_layouts() {
        let (o, _) = loaded(0);
        let (child, base, _) = ids(&o);
        let rows = word(&o, child + 48);
        let base_rows = word(&o, base + 48);
        let cell = word(&o, rows + 8);
        let base_cell = word(&o, base_rows + 40);
        for value in [0u32, 8, 12, 24, 64, 0x80000000, u32::MAX] {
            let (mut bad, r) = loaded(0);
            patch_bytes(&mut bad, cell, &value.to_le_bytes());
            reject(bad, r);
        }
        for (a, v) in [
            (rows, 24u32),
            (rows + 4, 0),
            (rows + 4, 9),
            (rows + 4, u32::MAX),
            (rows + 32, 2),
            (rows + 36, 4),
        ] {
            let (mut bad, r) = loaded(0);
            patch_bytes(&mut bad, a, &v.to_le_bytes());
            reject(bad, r);
        }
        for (a, v) in [
            (rows + 8, cell + 1),
            (rows + 8, cell + 2),
            (rows + 8, base),
            (rows + 8, base_cell),
            (rows + 16, word(&o, base_rows + 48)),
            (rows + 24, 0),
        ] {
            let (mut bad, r) = loaded(0);
            patch(&mut bad.image, a, v).unwrap();
            reject(bad, r);
        }
        let ty = word(&o, rows + 24);
        for byte in [b'@', b'q', b'[', b'^', b'#'] {
            let (mut bad, r) = loaded(0);
            patch_bytes(&mut bad, ty, &[byte]);
            reject(bad, r);
        }
        let (mut bad, r) = loaded(0);
        bad.objc_sections
            .parts
            .iter_mut()
            .find(|s| s.name == "__objc_ivar")
            .unwrap()
            .length -= 1;
        reject(bad, r);
        let (mut bad, r) = loaded(3);
        let (cr, _, _) = ids(&bad);
        let rows = word(&bad, cr + 48);
        let c = word(&bad, rows + 8);
        patch(&mut bad.image, rows + 40, c).unwrap();
        reject(bad, r);
    }
    #[test]
    fn layout_bounds_coverage_inherited_prefix_and_collection_order_are_checked() {
        for (profile, rs, cs) in [(0, 16, 24), (1, 8, 16), (2, 64, 64), (3, 16, 64)] {
            let (mut o, r) = loaded(profile);
            let (_, _, list) = ids(&o);
            let a = word(&o, list);
            let b = word(&o, list + 8);
            patch(&mut o.image, list, b).unwrap();
            patch(&mut o.image, list + 8, a).unwrap();
            let p = register(&mut [o], &[0], &[0], Some(r)).unwrap().unwrap();
            assert_eq!(
                p.classes
                    .iter()
                    .find(|c| c.superclass.is_none())
                    .unwrap()
                    .size,
                rs
            );
            assert_eq!(
                p.classes
                    .iter()
                    .find(|c| c.superclass.is_some())
                    .unwrap()
                    .size,
                cs
            );
        }
        let (o, _) = loaded(0);
        let (child, base, _) = ids(&o);
        for (a, v) in [
            (base + 4, 8u32),
            (base + 8, 7),
            (base + 8, 24),
            (base + 8, 72),
            (base + 8, u32::MAX),
            (child + 4, 0),
            (child + 4, 12),
            (child + 8, 16),
            (child + 8, 32),
            (child + 8, 65),
        ] {
            let (mut bad, r) = loaded(0);
            patch_bytes(&mut bad, a, &v.to_le_bytes());
            reject(bad, r);
        }
        let (mut bad, r) = loaded(2);
        let (child, _, _) = ids(&bad);
        patch_bytes(&mut bad, child + 4, &8u32.to_le_bytes());
        patch_bytes(&mut bad, child + 8, &8u32.to_le_bytes());
        reject(bad, r);
        assert!(scalar_signature("setValue:", "v24@0:8Q16", true).is_ok());
        assert!(scalar_signature("setValue:", "v24@0:8Q16", false).is_err());
        assert!(scalar_signature("setValue:", "v24@0:8v16", true).is_err());
    }
}
