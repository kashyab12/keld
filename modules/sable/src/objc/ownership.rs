//! Bounded ordinary ownership. No optimized return handoff or host objects.
use super::*;
use crate::objc::instances::{BASE, LENGTH, Lifetime};

pub(crate) const NAMES: [&str; 14] = [
    "_objc_autoreleasePoolPush",
    "_objc_autoreleasePoolPop",
    "_objc_retain",
    "_objc_release",
    "_objc_autorelease",
    "_objc_retainAutorelease",
    "_objc_claimAutoreleasedReturnValue",
    "_objc_retainAutoreleasedReturnValue",
    "_objc_release_x20",
    "_objc_release_x21",
    "_objc_release_x22",
    "_objc_release_x23",
    "_objc_release_x24",
    "_objc_retain_x22",
];
pub(crate) const fn word(index: usize) -> u32 {
    0xd4000001 | ((0x5ac0 + index as u32) << 5)
}
pub(crate) fn is_service(w: u32) -> bool {
    (0..NAMES.len()).any(|i| word(i) == w)
}
pub(crate) fn release_register(w: u32) -> Option<usize> {
    (8..NAMES.len())
        .find(|&i| word(i) == w)
        .map(|i| if i == 13 { 22 } else { i + 12 })
}
pub(crate) const RETURN: u64 = 0xffff_ffff_ffff_ffe0;
const TOKEN_BASE: u64 = 0xffff_0000_0000_0000;
const MAX_POOLS: usize = 32;
const MAX_QUEUE: usize = 1024;
const MAX_STRONG: u32 = 1_000_000;
#[derive(Clone, Copy, Debug)]
pub(crate) struct Runtime {
    pub entries: [u64; 14],
}
impl Runtime {
    fn index(self, pc: u64, w: u32) -> Result<usize> {
        (0..NAMES.len())
            .find(|&i| self.entries[i] == pc && word(i) == w)
            .ok_or_else(|| "unregistered ownership trap".into())
    }
    pub(crate) fn validate(self, image: &Image) -> Result<()> {
        let mut seen = BTreeSet::new();
        for (i, a) in self.entries.into_iter().enumerate() {
            if !seen.insert(a)
                || image.instruction(a)? != word(i)
                || image.instruction(a.checked_add(4).ok_or("ownership stub overflow")?)? != RET
            {
                return Err("invalid ownership runtime stub".into());
            }
        }
        for s in &image.segments {
            let end = s
                .address
                .checked_add(s.bytes.len() as u64)
                .ok_or("ownership range overflow")?;
            if (s.address <= RETURN && RETURN < end)
                || (s.address < TOKEN_BASE + (u32::MAX as u64) * 8 + 8 && TOKEN_BASE < end)
            {
                return Err("ownership private sentinel/token range overlaps image".into());
            }
        }
        if image.contains_pc(RETURN) {
            return Err("ownership sentinel is guest code".into());
        }
        Ok(())
    }
}
#[derive(Debug, PartialEq, Eq)]
struct Pool {
    token: u64,
    begin: usize,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct State {
    pools: Vec<Pool>,
    queue: Vec<u64>,
    next_token: u32,
    drain: Option<usize>,
    pub(crate) callback: Option<u64>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            pools: Vec::with_capacity(MAX_POOLS),
            queue: Vec::with_capacity(MAX_QUEUE),
            next_token: 0,
            drain: None,
            callback: None,
        }
    }
}
#[derive(Debug)]
pub(crate) enum Action {
    Return(u64),
    Dealloc {
        object: u64,
        selector: u64,
        imp: u64,
    },
}
impl Memory {
    /// This also checks the quarantined allocation ledger and immutable raw isa.
    fn owned_index(&self, a: u64) -> Result<usize> {
        self.instance_class(a)?
            .ok_or("ownership requires an exact owned instance")?;
        Ok((a - BASE) as usize / instances::SLOT_SIZE)
    }
    fn strong(&self, a: u64) -> Result<(usize, u32)> {
        let i = self.owned_index(a)?;
        match self.objects.as_ref().unwrap().rows[i].lifetime {
            Lifetime::Live(n) if n > 0 => Ok((i, n)),
            _ => Err("ownership of finalizing or retired object".into()),
        }
    }
    pub(crate) fn ownership_complete(&self) -> Result<()> {
        if let Some(s) = &self.ownership
            && (!s.pools.is_empty()
                || !s.queue.is_empty()
                || s.drain.is_some()
                || s.callback.is_some()
                || self
                    .objects
                    .as_ref()
                    .unwrap()
                    .rows
                    .iter()
                    .any(|r| r.lifetime != Lifetime::Retired))
        {
            return Err("unfinished ownership: live objects, pools or pending work".into());
        }
        Ok(())
    }
    pub(crate) fn ownership_busy(&self) -> bool {
        self.ownership
            .as_ref()
            .is_some_and(|s| s.drain.is_some() || s.callback.is_some())
    }
    pub(crate) fn check_final_disposal(&self, a: u64) -> Result<()> {
        if let Some(s) = &self.ownership {
            let i = self.owned_index(a)?;
            if s.callback != Some(a)
                || self.objects.as_ref().unwrap().rows[i].lifetime != Lifetime::Finalizing
            {
                return Err("ownership disposal requires expected finalizing callback".into());
            }
        }
        Ok(())
    }
    fn finish_dealloc(&mut self, a: u64) -> Result<()> {
        let s = self.ownership.as_mut().ok_or("ownership state absent")?;
        if s.callback != Some(a)
            || !(BASE..BASE + LENGTH as u64).contains(&a)
            || self.objects.as_ref().unwrap().rows[(a - BASE) as usize / instances::SLOT_SIZE]
                .lifetime
                != Lifetime::Retired
        {
            return Err("guest dealloc returned without exactly one final disposal".into());
        }
        s.callback = None;
        Ok(())
    }
}
impl Plan {
    pub(crate) fn ownership_enabled(&self) -> bool {
        self.runtime.ownership.is_some()
    }
    fn immortal(&self, a: u64) -> bool {
        a == 0 || self.constants.objects.contains(&a)
    }
    pub(crate) fn dealloc_target(&self, class: u64) -> Result<(u64, u64)> {
        let class = self
            .classes
            .iter()
            .find(|c| c.address == class)
            .ok_or("unregistered ownership class")?;
        let selector = self.dealloc_selector.ok_or("missing guest dealloc hook")?;
        let imp = self
            .method(class, false, selector)
            .map_err(|_| "missing guest dealloc hook")?;
        Ok((selector, imp))
    }
    fn preflight_pop(&self, memory: &Memory, token: u64) -> Result<usize> {
        let s = memory.ownership.as_ref().ok_or("ownership state absent")?;
        let pool = s
            .pools
            .last()
            .filter(|p| p.token == token)
            .ok_or("invalid or non-top autorelease pool token")?;
        let mut releases = [0u32; instances::SLOTS];
        for &a in s.queue.get(pool.begin..).ok_or("invalid drain boundary")? {
            let (i, n) = memory.strong(a)?;
            releases[i] = releases[i]
                .checked_add(1)
                .filter(|&v| v <= n)
                .ok_or("deferred releases exceed strong count")?;
            if releases[i] == n {
                self.dealloc_target(memory.objects.as_ref().unwrap().rows[i].class)?;
            }
        }
        Ok(pool.begin)
    }
    fn release_owned(&self, memory: &mut Memory, a: u64) -> Result<Action> {
        if self.immortal(a) {
            return Ok(Action::Return(0));
        }
        let (i, n) = memory.strong(a)?;
        if n > 1 {
            memory.objects.as_mut().unwrap().rows[i].lifetime = Lifetime::Live(n - 1);
            return Ok(Action::Return(0));
        }
        // Resolve and validate the hook before publishing the zero-count transition.
        let class = memory.objects.as_ref().unwrap().rows[i].class;
        let (selector, imp) = self.dealloc_target(class)?;
        if memory.ownership.as_ref().unwrap().callback.is_some() {
            return Err("lifecycle reentrancy unsupported".into());
        }
        memory.objects.as_mut().unwrap().rows[i].lifetime = Lifetime::Finalizing;
        memory.ownership.as_mut().unwrap().callback = Some(a);
        Ok(Action::Dealloc {
            object: a,
            selector,
            imp,
        })
    }
    pub(crate) fn finish_dealloc(&self, memory: &mut Memory, object: u64) -> Result<()> {
        self.check_memory(memory)?;
        memory.finish_dealloc(object)
    }
    pub(crate) fn drain_next(&self, memory: &mut Memory) -> Result<Action> {
        self.check_memory(memory)?;
        loop {
            let s = memory.ownership.as_ref().ok_or("ownership state absent")?;
            let Some(begin) = s.drain else {
                return Ok(Action::Return(0));
            };
            if s.callback.is_some() {
                return Err("lifecycle reentrancy unsupported".into());
            }
            if s.queue.len() == begin {
                let s = memory.ownership.as_mut().unwrap();
                s.pools.pop();
                s.drain = None;
                return Ok(Action::Return(0));
            }
            let a = *s.queue.last().ok_or("invalid drain boundary")?;
            // A failing release keeps this entry pending. No count is changed before hook checks.
            let action = self.release_owned(memory, a)?;
            memory.ownership.as_mut().unwrap().queue.pop();
            if matches!(action, Action::Dealloc { .. }) {
                return Ok(action);
            }
        }
    }
    pub(crate) fn ownership_trap(
        &self,
        memory: &mut Memory,
        pc: u64,
        w: u32,
        x0: u64,
        register_argument: u64,
    ) -> Result<Action> {
        let op = self
            .runtime
            .ownership
            .ok_or("unregistered ownership trap")?
            .index(pc, w)?;
        self.check_memory(memory)?;
        if memory.ownership_busy() {
            return Err("lifecycle reentrancy unsupported".into());
        }
        if op == 0 {
            let s = memory.ownership.as_mut().ok_or("ownership state absent")?;
            if s.pools.len() == MAX_POOLS {
                return Err("autorelease pool cap".into());
            }
            let generation = s
                .next_token
                .checked_add(1)
                .ok_or("autorelease token overflow")?;
            let token = TOKEN_BASE + u64::from(generation) * 8;
            s.pools.push(Pool {
                token,
                begin: s.queue.len(),
            });
            s.next_token = generation;
            return Ok(Action::Return(token));
        }
        if op == 1 {
            let begin = self.preflight_pop(memory, x0)?;
            memory.ownership.as_mut().unwrap().drain = Some(begin);
            return self.drain_next(memory);
        }
        let a = if op >= 8 { register_argument } else { x0 };
        if self.immortal(a) {
            return Ok(Action::Return(if matches!(op, 3 | 8..=12) { 0 } else { a }));
        }
        if matches!(op, 3 | 8..=12) {
            return self.release_owned(memory, a);
        }
        let (i, n) = memory.strong(a)?;
        let retain = matches!(op, 2 | 5 | 6 | 7 | 13);
        let enqueue = matches!(op, 4 | 5);
        let next = if retain {
            n.checked_add(1)
                .filter(|&v| v <= MAX_STRONG)
                .ok_or("strong count overflow")?
        } else {
            n
        };
        // Validate both halves before retain+enqueue: no partial count increment on failure.
        if enqueue {
            let s = memory.ownership.as_ref().unwrap();
            if s.pools.is_empty() {
                return Err("autorelease requires a pool".into());
            }
            if s.queue.len() == MAX_QUEUE {
                return Err("autorelease queue cap".into());
            }
        }
        memory.objects.as_mut().unwrap().rows[i].lifetime = Lifetime::Live(next);
        if enqueue {
            memory.ownership.as_mut().unwrap().queue.push(a);
        }
        Ok(Action::Return(a))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image() -> Image {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/bin/ownership/absolute/chained");
        let raw = std::fs::read(dir.join("client.macho")).unwrap();
        let libs = [
            "CoreFoundation.dylib",
            "Foundation.dylib",
            "libsable_objc.dylib",
            "libsable_system.dylib",
        ]
        .map(|n| std::fs::read(dir.join(n)).unwrap());
        Image::link_main(&raw, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap()
    }
    fn service(plan: &Plan, m: &mut Memory, op: usize, a: u64) -> Result<Action> {
        plan.ownership_trap(
            m,
            plan.runtime.ownership.unwrap().entries[op],
            word(op),
            a,
            a,
        )
    }
    fn scalar(plan: &Plan, m: &mut Memory, op: usize, a: u64) -> u64 {
        match service(plan, m, op, a).unwrap() {
            Action::Return(n) => n,
            _ => panic!("unexpected callback"),
        }
    }
    fn make(plan: &Plan, m: &mut Memory) -> u64 {
        let class = plan.classes.iter().find(|c| c.name == "NSString").unwrap();
        m.create_instance(class.address, 32, 64).unwrap()
    }
    fn snapshot(m: &Memory) -> String {
        format!("{:?}/{:?}", m.ownership, m.objects)
    }
    fn rejected_unchanged(plan: &Plan, m: &mut Memory, op: usize, a: u64) {
        let state = snapshot(m);
        let bytes = m.read(BASE, LENGTH).unwrap().to_vec();
        assert!(service(plan, m, op, a).is_err());
        assert_eq!(snapshot(m), state);
        assert_eq!(m.read(BASE, LENGTH).unwrap(), bytes);
    }
    #[test]
    fn continuation_boundaries_validate_metadata_before_ownership_effects() {
        let image = image();
        let plan = image.objc.as_ref().unwrap();
        let mut m = Memory::new(&image).unwrap();
        let token = scalar(plan, &mut m, 0, 0);
        let a = make(plan, &mut m);
        let b = make(plan, &mut m);
        scalar(plan, &mut m, 2, b);
        // Leave a second object's count decrements and disposal pending behind a.
        for object in [b, b, a] {
            scalar(plan, &mut m, 4, object);
        }
        assert!(
            matches!(service(plan, &mut m, 1, token).unwrap(), Action::Dealloc { object, .. } if object == a)
        );
        m.dispose_instance(a).unwrap();
        let metadata = plan
            .snapshots
            .iter()
            .map(|(address, _)| *address)
            .find(|&address| {
                m.regions.iter().any(|s| {
                    s.protection & 2 != 0
                        && s.address <= address
                        && address < s.address + s.bytes.len() as u64
                })
            })
            .expect("writable metadata snapshot");
        let mutations = [
            (metadata, "mutated ObjC metadata"),
            (plan.runtime.empty_cache, "mutated ObjC cache"),
        ];
        for (address, expected) in mutations {
            let original = m.read(address, 1).unwrap()[0];
            m.write(address, &[original ^ 1]).unwrap();
            let before = snapshot(&m);
            let arena = m.read(BASE, LENGTH).unwrap().to_vec();
            assert!(
                plan.finish_dealloc(&mut m, a)
                    .unwrap_err()
                    .contains(expected)
            );
            assert_eq!(snapshot(&m), before);
            assert_eq!(m.read(BASE, LENGTH).unwrap(), arena);
            m.write(address, &[original]).unwrap();
        }
        plan.finish_dealloc(&mut m, a).unwrap();
        // Check both a nonempty drain and the final pool-removal boundary.
        for pending_object in [Some(b), None] {
            for (address, expected) in mutations {
                let original = m.read(address, 1).unwrap()[0];
                m.write(address, &[original ^ 1]).unwrap();
                let before = snapshot(&m);
                let arena = m.read(BASE, LENGTH).unwrap().to_vec();
                assert!(plan.drain_next(&mut m).unwrap_err().contains(expected));
                assert_eq!(snapshot(&m), before);
                assert_eq!(m.read(BASE, LENGTH).unwrap(), arena);
                m.write(address, &[original]).unwrap();
            }
            if let Some(object) = pending_object {
                assert!(
                    matches!(plan.drain_next(&mut m).unwrap(), Action::Dealloc { object: actual, .. } if actual == object)
                );
                m.dispose_instance(object).unwrap();
                plan.finish_dealloc(&mut m, object).unwrap();
            } else {
                assert!(matches!(
                    plan.drain_next(&mut m).unwrap(),
                    Action::Return(0)
                ));
            }
        }
        m.ownership_complete().unwrap();
    }
    #[test]
    fn atomic_counts_caps_tokens_and_exact_owned_or_immortal_identities() {
        let image = image();
        let plan = image.objc.as_ref().unwrap();
        let mut m = Memory::new(&image).unwrap();
        let a = make(plan, &mut m);
        rejected_unchanged(plan, &mut m, 5, a); // retain+enqueue without pool
        for invalid in [1, a + 8, a + 128, plan.classes[0].address] {
            for op in [2, 3, 4, 5, 6, 7, 8] {
                rejected_unchanged(plan, &mut m, op, invalid);
            }
        }
        let literal = *plan.constants.objects.first().unwrap();
        for op in [2, 3, 4, 5, 6, 7, 8] {
            scalar(plan, &mut m, op, 0);
            scalar(plan, &mut m, op, literal);
        }
        assert_eq!(
            m.objects.as_ref().unwrap().rows[0].lifetime,
            Lifetime::Live(1)
        );
        let token = scalar(plan, &mut m, 0, 0);
        let pool_storage = m.ownership.as_ref().unwrap().pools.as_ptr();
        let queue_storage = m.ownership.as_ref().unwrap().queue.as_ptr();
        let ledger = m.objects.as_ref().unwrap().rows.as_ptr();
        let arena = m
            .regions
            .iter()
            .find(|r| r.address == BASE)
            .unwrap()
            .bytes
            .as_ptr();
        scalar(plan, &mut m, 5, a);
        assert_eq!(
            m.objects.as_ref().unwrap().rows[0].lifetime,
            Lifetime::Live(2)
        );
        assert_eq!(m.ownership.as_ref().unwrap().queue, [a]);
        for _ in 1..MAX_QUEUE {
            scalar(plan, &mut m, 5, a);
        }
        rejected_unchanged(plan, &mut m, 5, a);
        m.ownership.as_mut().unwrap().queue.clear();
        m.objects.as_mut().unwrap().rows[0].lifetime = Lifetime::Live(MAX_STRONG);
        for op in [2, 5, 6, 7, 13] {
            rejected_unchanged(plan, &mut m, op, a);
        }
        for _ in 1..MAX_POOLS {
            scalar(plan, &mut m, 0, 0);
        }
        rejected_unchanged(plan, &mut m, 0, 0);
        rejected_unchanged(plan, &mut m, 1, token); // non-top
        rejected_unchanged(plan, &mut m, 1, token + 1);
        for _ in 0..MAX_POOLS {
            let t = m.ownership.as_ref().unwrap().pools.last().unwrap().token;
            scalar(plan, &mut m, 1, t);
        }
        rejected_unchanged(plan, &mut m, 1, token); // stale
        m.ownership.as_mut().unwrap().next_token = u32::MAX;
        rejected_unchanged(plan, &mut m, 0, 0);
        assert_eq!(m.ownership.as_ref().unwrap().pools.as_ptr(), pool_storage);
        assert_eq!(m.ownership.as_ref().unwrap().queue.as_ptr(), queue_storage);
        assert_eq!(m.objects.as_ref().unwrap().rows.as_ptr(), ledger);
        assert_eq!(
            m.regions
                .iter()
                .find(|r| r.address == BASE)
                .unwrap()
                .bytes
                .as_ptr(),
            arena
        );
    }
    #[test]
    fn zero_count_requires_hook_finalizing_callback_and_single_scrubbed_disposal() {
        let image = image();
        let plan = image.objc.as_ref().unwrap();
        let mut m = Memory::new(&image).unwrap();
        let a = make(plan, &mut m);
        assert!(m.dispose_instance(a).is_err());
        let mut missing = plan.clone();
        missing.dealloc_selector = None;
        rejected_unchanged(&missing, &mut m, 3, a);
        let t = scalar(plan, &mut m, 0, 0);
        scalar(plan, &mut m, 5, a);
        scalar(plan, &mut m, 3, a);
        let Action::Dealloc {
            object,
            selector,
            imp,
        } = service(plan, &mut m, 1, t).unwrap()
        else {
            panic!("missing callback")
        };
        assert_eq!(object, a);
        assert!(image.contains_pc(imp));
        assert_eq!(selector, plan.dealloc_selector.unwrap());
        assert_eq!(
            m.objects.as_ref().unwrap().rows[0].lifetime,
            Lifetime::Finalizing
        );
        assert!(plan.finish_dealloc(&mut m, a).is_err());
        for op in 0..NAMES.len() {
            assert!(service(plan, &mut m, op, a).is_err());
        }
        let r = plan.runtime.instances.unwrap();
        assert!(
            plan.trap(
                &mut m,
                r.create,
                instances::CREATE,
                plan.classes[0].address,
                0,
                0
            )
            .is_err()
        );
        assert!(
            plan.trap(&mut m, plan.runtime.msg_send + 4, LOOKUP, a, selector, 0)
                .is_err()
        );
        let other = make(plan, &mut m); // low-level test setup only; guest CREATE is blocked while busy
        assert!(m.dispose_instance(other).is_err());
        m.write(a + 8, &[0xa5; 64]).unwrap();
        m.dispose_instance(a).unwrap();
        assert!(
            m.read(a, instances::SLOT_SIZE)
                .unwrap()
                .iter()
                .all(|&v| v == 0)
        );
        assert!(m.dispose_instance(a).is_err());
        assert!(m.instance_class(a).is_err());
        plan.finish_dealloc(&mut m, a).unwrap();
        assert!(plan.finish_dealloc(&mut m, a).is_err());
        assert!(matches!(
            plan.drain_next(&mut m).unwrap(),
            Action::Return(0)
        ));
        for op in [2, 3, 4, 5] {
            rejected_unchanged(plan, &mut m, op, a);
        }
        assert!(m.ownership_complete().is_err()); // deliberately live `other`
        // Forging an isa back into a quarantined row cannot resurrect it.
        m.write(a, &plan.classes[0].address.to_le_bytes()).unwrap();
        assert!(m.instance_class(a).is_err());
    }
}

#[cfg(test)]
mod fallback_trace_tests {
    use super::*;
    #[test]
    fn ordinary_minimal_count_trace_and_invalid_drain_preflight() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/bin/ownership/absolute/chained");
        let raw = std::fs::read(dir.join("client.macho")).unwrap();
        let libs = [
            "CoreFoundation.dylib",
            "Foundation.dylib",
            "libsable_objc.dylib",
            "libsable_system.dylib",
        ]
        .map(|n| std::fs::read(dir.join(n)).unwrap());
        let image =
            Image::link_main(&raw, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
        let plan = image.objc.as_ref().unwrap();
        let mut memory = Memory::new(&image).unwrap();
        let service = |m: &mut Memory, op: usize, a: u64| {
            plan.ownership_trap(
                m,
                plan.runtime.ownership.unwrap().entries[op],
                word(op),
                a,
                a,
            )
        };
        let Action::Return(token) = service(&mut memory, 0, 0).unwrap() else {
            panic!("pool token")
        };
        let class = plan
            .classes
            .iter()
            .find(|c| c.name == "NSString")
            .unwrap()
            .address;
        let a = memory.create_instance(class, 32, 17).unwrap();
        let check = |m: &Memory, n, q| {
            assert_eq!(
                m.objects.as_ref().unwrap().rows[0].lifetime,
                Lifetime::Live(n)
            );
            assert_eq!(m.ownership.as_ref().unwrap().queue.len(), q);
        };
        check(&memory, 1, 0);
        service(&mut memory, 4, a).unwrap();
        check(&memory, 1, 1);
        service(&mut memory, 7, a).unwrap();
        check(&memory, 2, 1);
        service(&mut memory, 5, a).unwrap();
        check(&memory, 3, 2);
        service(&mut memory, 8, a).unwrap();
        check(&memory, 2, 2);
        // A third queued release would underflow: pool pop validates all work before changing any state.
        memory.ownership.as_mut().unwrap().queue.push(a);
        let state = format!("{:?}/{:?}", memory.objects, memory.ownership);
        assert!(service(&mut memory, 1, token).is_err());
        assert_eq!(
            format!("{:?}/{:?}", memory.objects, memory.ownership),
            state
        );
        memory.ownership.as_mut().unwrap().queue.pop();
        let Action::Dealloc { object, .. } = service(&mut memory, 1, token).unwrap() else {
            panic!("final callback")
        };
        assert_eq!(object, a);
        memory.dispose_instance(a).unwrap();
        plan.finish_dealloc(&mut memory, a).unwrap();
        plan.drain_next(&mut memory).unwrap();
        memory.ownership_complete().unwrap();
        // A freshly created live row with an invalid count is never treated as an immortal or zero release.
        let b = memory.create_instance(class, 32, 0).unwrap();
        memory.objects.as_mut().unwrap().rows[1].lifetime = Lifetime::Live(0);
        for op in [2, 3, 4, 5, 6, 7, 8] {
            assert!(service(&mut memory, op, b).is_err());
        }
    }
}
