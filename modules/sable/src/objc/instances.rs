//! Fixed-layout guest instances with quarantined identities and stable backing.
use crate::{
    Result,
    macho::Image,
    memory::{Memory, STACK_BASE, STACK_SIZE},
};
pub const BASE: u64 = 0x7200_0000;
pub const SLOTS: usize = 256;
pub const SLOT_SIZE: usize = 128;
pub const LENGTH: usize = SLOTS * SLOT_SIZE;
pub const MAX_EXTRA: u64 = 64;
pub(crate) const CREATE: u32 = 0xd4000001 | (0x5ab8 << 5);
pub(crate) const DISPOSE: u32 = 0xd4000001 | (0x5ab9 << 5);
#[derive(Clone, Copy, Debug)]
pub(crate) struct Runtime {
    pub create: u64,
    pub dispose: u64,
}
pub(crate) fn is_service(word: u32) -> bool {
    matches!(word, CREATE | DISPOSE)
}
pub(crate) fn registered_trap(runtime: Option<Runtime>, pc: u64, word: u32) -> bool {
    runtime
        .is_some_and(|r| (word == CREATE && pc == r.create) || (word == DISPOSE && pc == r.dispose))
}
pub(crate) fn validate_runtime(image: &Image, runtime: Option<Runtime>) -> Result<()> {
    if let Some(r) = runtime {
        for (a, word) in [(r.create, CREATE), (r.dispose, DISPOSE)] {
            if image.instruction(a)? != word
                || image.instruction(a.checked_add(4).ok_or("instance stub overflow")?)?
                    != 0xd65f03c0
            {
                return Err("invalid instance runtime stub".into());
            }
        }
    }
    Ok(())
}
pub(crate) fn layout(image: &Image, tls_length: usize) -> Result<usize> {
    if !image.objc.as_ref().is_some_and(|p| p.instances_enabled()) {
        return Ok(0);
    }
    let overlap = |a: u64, n: usize| -> Result<bool> {
        let end = a
            .checked_add(n as u64)
            .ok_or("instance layout range overflow")?;
        Ok(n != 0 && a < BASE + LENGTH as u64 && BASE < end)
    };
    let mut total = LENGTH + STACK_SIZE + tls_length;
    for s in &image.segments {
        if overlap(s.address, s.bytes.len())? {
            return Err("instance arena overlaps image".into());
        }
        total = total
            .checked_add(s.bytes.len())
            .ok_or("instance mapped size overflow")?;
    }
    if overlap(STACK_BASE, STACK_SIZE)?
        || overlap(crate::tls::BASE, tls_length)?
        || total > 64 * 1024 * 1024
    {
        return Err("instance arena overlaps reserved storage or exceeds memory cap".into());
    }
    Ok(LENGTH)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lifetime {
    Live(u32),
    Finalizing,
    Retired,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Allocation {
    pub class: u64,
    pub extent: usize,
    pub lifetime: Lifetime,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Allocations {
    pub rows: Vec<Allocation>,
}
impl Default for Allocations {
    fn default() -> Self {
        Self {
            rows: Vec::with_capacity(SLOTS),
        }
    }
}
impl Memory {
    pub(crate) fn instance_class(&self, address: u64) -> Result<Option<u64>> {
        let Some(objects) = &self.objects else {
            return Ok(None);
        };
        if !(BASE..BASE + LENGTH as u64).contains(&address) {
            return Ok(None);
        }
        let offset = (address - BASE) as usize;
        if !offset.is_multiple_of(SLOT_SIZE) {
            return Err("interior owned instance identity".into());
        }
        let record = objects
            .rows
            .get(offset / SLOT_SIZE)
            .filter(|r| r.lifetime != Lifetime::Retired)
            .ok_or("unallocated or disposed instance identity")?;
        if u64::from_le_bytes(self.read(address, 8)?.try_into().unwrap()) != record.class {
            return Err("mutated owned instance isa".into());
        }
        Ok(Some(record.class))
    }
    pub(crate) fn create_instance(&mut self, class: u64, size: usize, extra: u64) -> Result<u64> {
        if !(8..=64).contains(&size) || !size.is_multiple_of(8) {
            return Err("unsupported instance class size".into());
        }
        if extra > MAX_EXTRA {
            return Err("instance extra bytes exceed supported cap".into());
        }
        let count = self
            .objects
            .as_ref()
            .ok_or("instance arena absent")?
            .rows
            .len();
        if count == SLOTS {
            return Ok(0);
        }
        let extent = size
            .checked_add(extra as usize)
            .filter(|&span| span <= SLOT_SIZE)
            .ok_or("instance size overflow or slot cap")?;
        let address = BASE + (count * SLOT_SIZE) as u64;
        self.write(address, &[0; SLOT_SIZE])?;
        self.write(address, &class.to_le_bytes())?;
        self.objects.as_mut().unwrap().rows.push(Allocation {
            class,
            extent,
            lifetime: Lifetime::Live(1),
        });
        Ok(address)
    }
    pub(crate) fn dispose_instance(&mut self, address: u64) -> Result<u64> {
        self.instance_class(address)?
            .ok_or("disposal requires an owned instance")?;
        self.check_final_disposal(address)?;
        self.write(address, &[0; SLOT_SIZE])?;
        self.objects.as_mut().unwrap().rows[(address - BASE) as usize / SLOT_SIZE].lifetime =
            Lifetime::Retired;
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image() -> Image {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/bin/instances/absolute/chained/0");
        let raw = std::fs::read(root.join("instance-0.macho")).unwrap();
        let libs = ["libsable_objc.dylib", "libsable_system.dylib"]
            .map(|n| std::fs::read(root.join(n)).unwrap());
        Image::link(&raw, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap()
    }
    #[test]
    fn object_signature_subset_retains_exact_counts_offsets_and_unsigned_protocol_profile() {
        for (name, ty) in [
            ("make:", "@24@0:8Q16"),
            ("identity", "@16@0:8"),
            ("matches:", "Q24@0:8@16"),
            ("choose:", "@24@0:8@16"),
            ("setValue:", "v24@0:8Q16"),
        ] {
            assert!(super::super::scalar_signature(name, ty, true).is_ok());
            assert!(super::super::scalar_signature(name, ty, false).is_err());
        }
        assert!(super::super::scalar_signature("value:", "Q24@0:8Q16", false).is_ok());
        for ty in [
            "#24@0:8Q16",
            ":24@0:8Q16",
            "^@24@0:8Q16",
            "r@24@0:8Q16",
            "@024@0:8Q16",
            "@24@0:8Q016",
            "@24@0:8Q8",
            "@24@0:8Q16Q24",
            "@24@0:8#16",
            "@24@0:8:16",
            "@24@0:8d16",
            "@24@0:8@\"Thing\"16",
            "@24@0:8^v16",
            "@24@0:8Q18446744073709551616",
        ] {
            assert!(
                super::super::scalar_signature("value:", ty, true).is_err(),
                "{ty}"
            );
        }
        for name in ["load", "initialize", "a:b:c:d:e:f:g:"] {
            assert!(super::super::scalar_signature(name, "@16@0:8", true).is_err());
        }
    }
    #[test]
    fn lifetime_capacity_zeroing_and_backing_addresses_are_private_and_stable() {
        let image = image();
        let class = image
            .objc
            .as_ref()
            .unwrap()
            .classes
            .iter()
            .find(|c| c.name == "AllocChild")
            .unwrap()
            .address;
        let mut m = Memory::new(&image).unwrap();
        let initial_layout = m.layout();
        let host = m
            .regions
            .iter()
            .find(|s| s.address == BASE)
            .unwrap()
            .bytes
            .as_ptr();
        let ledger = m.objects.as_ref().unwrap().rows.as_ptr();
        m.write(BASE, &[0xa5; SLOT_SIZE]).unwrap();
        let a = m.create_instance(class, 8, 64).unwrap();
        assert_eq!(a, BASE);
        assert_eq!(m.instance_class(a).unwrap(), Some(class));
        assert!(
            m.read(a + 8, SLOT_SIZE - 8)
                .unwrap()
                .iter()
                .all(|&v| v == 0)
        );
        m.write(a + 8, &[0x5a; 64]).unwrap();
        assert!(m.instance_class(a + 8).is_err());
        let forged = BASE + SLOT_SIZE as u64;
        m.write(forged, &class.to_le_bytes()).unwrap();
        assert!(m.instance_class(forged).is_err());
        assert_eq!(m.dispose_instance(a).unwrap(), 0);
        assert!(m.read(a, SLOT_SIZE).unwrap().iter().all(|&v| v == 0));
        m.write(a, &class.to_le_bytes()).unwrap();
        assert!(m.instance_class(a).is_err());
        assert!(m.dispose_instance(a).is_err());
        assert!(m.dispose_instance(class).is_err());
        for n in 1..SLOTS {
            let p = m.create_instance(class, 8, (n % 65) as u64).unwrap();
            assert_eq!(p, BASE + (n * SLOT_SIZE) as u64);
            assert!(p.is_multiple_of(16));
            assert!(
                m.read(p + 8, SLOT_SIZE - 8)
                    .unwrap()
                    .iter()
                    .all(|&v| v == 0)
            );
        }
        let before = m.read(BASE, LENGTH).unwrap().to_vec();
        assert_eq!(m.create_instance(class, 8, 0).unwrap(), 0);
        for extra in [65, u64::MAX] {
            assert!(m.create_instance(class, 8, extra).is_err());
        }
        assert_eq!(m.read(BASE, LENGTH).unwrap(), before);
        assert_eq!(m.objects.as_ref().unwrap().rows.len(), SLOTS);
        assert_eq!(m.objects.as_ref().unwrap().rows.as_ptr(), ledger);
        assert_eq!(
            m.regions
                .iter()
                .find(|s| s.address == BASE)
                .unwrap()
                .bytes
                .as_ptr(),
            host
        );
        assert_eq!(m.layout(), initial_layout);
        let last = BASE + ((SLOTS - 1) * SLOT_SIZE) as u64;
        m.write(last, &0u64.to_le_bytes()).unwrap();
        assert!(m.instance_class(last).is_err());
        assert!(m.dispose_instance(last).is_err());
        let mut fresh = Memory::new(&image).unwrap();
        assert_eq!(fresh.create_instance(class, 8, 64).unwrap(), BASE);
        assert!(fresh.read(BASE + 8, 64).unwrap().iter().all(|&v| v == 0));
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            assert_eq!(Memory::prepared_layout(&image).unwrap(), fresh.layout());
            let before = fresh.pointers().unwrap();
            let b = fresh.create_instance(class, 8, 16).unwrap();
            fresh.dispose_instance(b).unwrap();
            assert_eq!(before, fresh.pointers().unwrap());
        }
    }
    #[test]
    fn arena_layout_checks_images_reserved_ranges_and_total_cap() {
        let i = image();
        let m = Memory::new(&i).unwrap();
        let mut ranges = m.layout();
        ranges.sort();
        for pair in ranges.windows(2) {
            assert!(pair[0].0 + pair[0].1 as u64 <= pair[1].0);
        }
        assert!(ranges.iter().any(|r| r.0 == crate::tls::BASE));
        assert!(ranges.iter().any(|r| r.0 == BASE && r.1 == LENGTH));
        for address in [BASE - 8, BASE, BASE + LENGTH as u64 - 8] {
            let mut bad = i.clone();
            bad.segments.push(crate::macho::Segment {
                address,
                bytes: vec![0; 16].into(),
                protection: 3,
            });
            assert!(
                Memory::new(&bad)
                    .unwrap_err()
                    .contains("instance arena overlaps")
            );
            #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
            assert!(Memory::prepared_layout(&bad).is_err());
        }
        let mut bad = i;
        bad.tls.clear();
        bad.segments.push(crate::macho::Segment {
            address: 0x90000000,
            bytes: vec![0; 64 * 1024 * 1024].into(),
            protection: 1,
        });
        assert!(Memory::new(&bad).unwrap_err().contains("memory cap"));
        let old = Image::parse(include_bytes!("../../fixtures/bin/hello.macho")).unwrap();
        assert!(
            !Memory::new(&old)
                .unwrap()
                .layout()
                .iter()
                .any(|r| r.0 == BASE)
        );
    }
}
