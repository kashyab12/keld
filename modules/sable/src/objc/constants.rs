//! Original checked ASCII constant records. No host strings or Foundation methods.
use super::*;
pub(crate) const CORE_FOUNDATION: &str =
    "/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation";
pub(crate) const FOUNDATION: &str = "/System/Library/Frameworks/Foundation.framework/Foundation";
#[derive(Clone, Copy, Debug)]
pub(crate) struct Provider {
    pub index: usize,
    pub foundation: usize,
    pub class: u64,
}
#[derive(Clone, Debug, Default)]
pub(super) struct Registry {
    pub readonly: Vec<(u64, usize)>,
    pub objects: BTreeSet<u64>,
}
pub(super) fn validate(
    objects: &[Object],
    active: &[usize],
    slides: &[u64],
    provider: Option<Provider>,
    classes: &[Pending],
) -> Result<Registry> {
    if !active
        .iter()
        .any(|&i| objects[i].objc_sections.has_constants())
    {
        return Ok(Registry::default());
    }
    let p = provider.ok_or("missing constant-string provider")?;
    let cf = objects
        .get(p.index)
        .ok_or("invalid constant provider index")?;
    let foundation = objects
        .get(p.foundation)
        .ok_or("invalid string provider index")?;
    if cf.id.as_deref() != Some(CORE_FOUNDATION)
        || foundation.id.as_deref() != Some(FOUNDATION)
        || !cf.dependencies.iter().any(|(n, _)| n == FOUNDATION)
    {
        return Err("invalid constant framework dependency graph".into());
    }
    record(cf, slides[p.index], "__objc_data", p.class, 40)?;
    let child = classes
        .iter()
        .find(|c| c.address == p.class)
        .ok_or("constant alias is not a registered ordinary class")?;
    let root = classes
        .iter()
        .find(|c| c.address == child.superclass)
        .ok_or("constant class lacks registered string parent")?;
    record(
        foundation,
        slides[p.foundation],
        "__objc_data",
        root.address,
        40,
    )?;
    if root.name != "NSString"
        || root.superclass != 0
        || root.layout.size != 32
        || child.layout.start != 32
        || child.layout.size != 32
        || !child.layout.names.is_empty()
        || !root.instance.iter().any(|(n, _)| n == "UTF8String")
    {
        return Err("unsupported constant-string class/layout/getter profile".into());
    }
    let mut count = 0;
    let mut registry = Registry::default();
    for &i in active {
        let o = &objects[i];
        let Some((base, len)) = o.objc_sections.section("__cfstring", slides[i])? else {
            continue;
        };
        if !o.dependencies.iter().any(|(n, _)| n == CORE_FOUNDATION) {
            return Err("constant consumer lacks CoreFoundation dependency".into());
        }
        count += len / 32;
        if count > 256 {
            return Err("constant-string count exceeds cap".into());
        }
        // SG_READ_ONLY has already been applied after eager fixups. Require the
        // entire owning segment to be immutable, not just a mutable snapshot.
        if o.image.segments.iter().any(|s| {
            s.protection & 2 != 0 && base >= s.address && base < s.address + s.bytes.len() as u64
        }) {
            return Err("constant-string storage must be read-only after fixups".into());
        }
        registry.readonly.push((base, len));
        for at in (0..len).step_by(32) {
            if !registry.objects.insert(base + at as u64) {
                return Err("duplicate constant-string object identity".into());
            }
            registry
                .readonly
                .push(validate_record(o, slides[i], base + at as u64, p.class)?);
        }
    }
    Ok(registry)
}

fn validate_record(o: &Object, slide: u64, base: u64, class: u64) -> Result<(u64, usize)> {
    let row = record(o, slide, "__cfstring", base, 32)?;
    let address = u64_at(row, 16)?;
    let length = u64_at(row, 24)?;
    if u64_at(row, 0)? != class || u32_at(row, 8)? != 0x7c8 || u32_at(row, 12)? != 0 || length > 63
    {
        return Err("unsupported constant-string isa/flags/padding/length".into());
    }
    let size = length as usize + 1;
    o.objc_sections
        .contains("__cstring", slide, address, size)?;
    let bytes = o.image.read(address, size, 1)?;
    if bytes[size - 1] != 0 || bytes[..size - 1].iter().any(|&c| c == 0 || c >= 128) {
        return Err("invalid constant-string ASCII bytes/terminator".into());
    }
    if o.image.read(address, size, 3).is_ok() {
        return Err("constant backing bytes must be read-only".into());
    }
    Ok((address, size))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn post_fixup_constant_fields_and_owning_ascii_bytes_reject_malformed_records() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/bin/strings/absolute/chained");
        let raw = std::fs::read(dir.join("strings-0.macho")).unwrap();
        let libs = [
            "libsable_literals.dylib",
            "CoreFoundation.dylib",
            "Foundation.dylib",
            "libsable_system.dylib",
            "libsable_objc.dylib",
        ]
        .map(|n| std::fs::read(dir.join(n)).unwrap());
        let image = Image::link(&raw, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
        let make = || {
            let mut object = Object::parse(&raw, true).unwrap();
            for s in &mut object.image.segments {
                *s = image
                    .segments
                    .iter()
                    .find(|p| p.address == s.address)
                    .unwrap()
                    .clone();
            }
            object
        };
        let o = make();
        let (base, len) = o.objc_sections.section("__cfstring", 0).unwrap().unwrap();
        for at in (0..len).step_by(32) {
            let row = base + at as u64;
            validate_record(
                &o,
                0,
                row,
                u64_at(o.image.read(row, 8, 1).unwrap(), 0).unwrap(),
            )
            .unwrap();
        }
        let row = (0..len)
            .step_by(32)
            .map(|n| base + n as u64)
            .find(|&a| u64_at(o.image.read(a + 24, 8, 1).unwrap(), 0).unwrap() == 16)
            .unwrap();
        let class = u64_at(o.image.read(row, 8, 1).unwrap(), 0).unwrap();
        let bytes = u64_at(o.image.read(row + 16, 8, 1).unwrap(), 0).unwrap();
        for (a, v) in [
            (row, 0),
            (row, class + 8),
            (row, row),
            (row + 8, 0),
            (row + 8, 0x7d0),
            (row + 8, (1u64 << 32) | 0x7c8),
            (row + 16, 0),
            (row + 16, row),
            (row + 16, bytes + 1),
            (row + 24, 15),
            (row + 24, 17),
            (row + 24, 64),
            (row + 24, u64::MAX),
        ] {
            let mut bad = make();
            patch(&mut bad.image, a, v).unwrap();
            assert!(validate_record(&bad, 0, row, class).is_err());
        }
        for (at, value) in [(bytes, 0), (bytes, 128), (bytes, 255), (bytes + 16, 1)] {
            let mut bad = make();
            let segment = bad
                .image
                .segments
                .iter_mut()
                .find(|s| at >= s.address && at < s.address + s.bytes.len() as u64)
                .unwrap();
            std::sync::Arc::make_mut(&mut segment.bytes)[(at - segment.address) as usize] = value;
            assert!(validate_record(&bad, 0, row, class).is_err());
        }
        assert!(scalar_signature("UTF8String", "r*16@0:8", true).is_ok());
        for (name, encoding) in [
            ("UTF8String", "r*16@0:8"),
            ("pointer:", "r*24@0:8Q16"),
            ("pointer", "*16@0:8"),
            ("pointer", "^c16@0:8"),
            ("load", "r*16@0:8"),
        ] {
            assert!(scalar_signature(name, encoding, false).is_err());
            if name != "UTF8String" {
                assert!(scalar_signature(name, encoding, true).is_err());
            }
        }
    }
}
