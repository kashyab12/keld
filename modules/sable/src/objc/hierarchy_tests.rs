use super::*;
fn loaded() -> (Object, Runtime) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/bin/super/absolute/chained/0");
    let raw = std::fs::read(root.join("super-0.macho")).unwrap();
    let libs = ["libsable_objc.dylib", "libsable_system.dylib"]
        .map(|n| std::fs::read(root.join(n)).unwrap());
    let image = Image::link(&raw, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
    let runtime = image.objc.as_ref().unwrap().runtime;
    let mut object = Object::parse(&raw, true).unwrap();
    for segment in &mut object.image.segments {
        *segment = image
            .segments
            .iter()
            .find(|s| s.address == segment.address)
            .unwrap()
            .clone();
    }
    (object, runtime)
}
fn word(o: &Object, a: u64) -> u64 {
    u64_at(o.image.read(a, 8, 1).unwrap(), 0).unwrap()
}
fn overwrite(o: &mut Object, a: u64, data: &[u8]) {
    for s in &mut o.image.segments {
        if let Some(n) = a.checked_sub(s.address).map(|v| v as usize)
            && n.checked_add(data.len())
                .is_some_and(|end| end <= s.bytes.len())
        {
            std::sync::Arc::make_mut(&mut s.bytes)[n..n + data.len()].copy_from_slice(data);
            return;
        }
    }
    panic!("invalid test patch");
}
fn identities(o: &Object) -> (u64, u64, u64, u64, u64) {
    let (list, len) = o
        .objc_sections
        .section("__objc_classlist", 0)
        .unwrap()
        .unwrap();
    assert_eq!(len, 16);
    let rows = [word(o, list), word(o, list + 8)];
    let child = *rows.iter().find(|&&a| word(o, a + 8) != 0).unwrap();
    let base = word(o, child + 8);
    (child, word(o, child), base, word(o, base), list)
}
fn rejects(o: Object, r: Runtime) {
    assert!(register(&mut [o], &[0], &[0], Some(r)).is_err());
}
#[test]
fn superclass_graph_collection_is_order_independent_and_malformed_edges_reject() {
    let (mut good, r) = loaded();
    let (child, meta, base, bmeta, list) = identities(&good);
    let a = word(&good, list);
    let b = word(&good, list + 8);
    patch(&mut good.image, list, b).unwrap();
    patch(&mut good.image, list + 8, a).unwrap();
    let plan = register(&mut [good], &[0], &[0], Some(r)).unwrap().unwrap();
    assert_eq!(
        plan.classes
            .iter()
            .find(|c| c.address == child)
            .unwrap()
            .superclass,
        Some(base)
    );
    for (address, value) in [
        (child + 8, child),
        (child + 8, meta),
        (child + 8, bmeta),
        (child + 8, base + 8),
        (child + 8, u64::MAX),
        (base + 8, child),
        (meta, meta),
        (meta, child),
        (meta + 8, base),
        (meta + 8, 0),
        (bmeta, meta),
        (bmeta + 8, 0),
        (bmeta + 8, meta),
    ] {
        let (mut bad, r) = loaded();
        patch(&mut bad.image, address, value).unwrap();
        rejects(bad, r);
    }
    let (bad, mut r) = loaded();
    r.super_send = None;
    rejects(bad, r);
}
#[test]
fn subclass_layout_superrefs_and_owning_records_reject_unsupported_forms() {
    let (good, _) = loaded();
    let (child, meta, base, _, _) = identities(&good);
    let ro = word(&good, child + 32);
    let mr = word(&good, meta + 32);
    for (a, v) in [
        (ro, 2u32),
        (ro + 4, 0),
        (ro + 8, 16),
        (ro + 12, 1),
        (mr, 3),
        (mr + 4, 8),
        (mr + 8, 8),
    ] {
        let (mut bad, r) = loaded();
        overwrite(&mut bad, a, &v.to_le_bytes());
        rejects(bad, r);
    }
    let base_ro = word(&good, base + 32);
    let base_ivars = word(&good, base_ro + 48);
    for (a, v) in [(ro + 48, base_ivars), (child + 32, ro + 8), (meta + 32, ro)] {
        let (mut bad, r) = loaded();
        patch(&mut bad.image, a, v).unwrap();
        rejects(bad, r);
    }
    let refs = good
        .objc_sections
        .section("__objc_superrefs", 0)
        .unwrap()
        .unwrap()
        .0;
    for target in [0, child + 8, ro, u64::MAX] {
        let (mut bad, r) = loaded();
        patch(&mut bad.image, refs, target).unwrap();
        rejects(bad, r);
    }
}
