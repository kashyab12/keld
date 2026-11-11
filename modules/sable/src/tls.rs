//! Bounded, single-context Darwin scalar TLV. No pthreads or host TLS pointers.
use crate::{
    Result,
    macho::{Image, Object, u32_at, u64_at},
};
use std::sync::Arc;

pub const BASE: u64 = 0x7100_0000;
pub const LIMIT: usize = 1024 * 1024;
/// Sable-private guest resolver trap, not a Darwin service number.
pub const SVC: u32 = 0xd400_0001 | (0x5ab1 << 5);
const RET: u32 = 0xd65f_03c0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Descriptor {
    pub address: u64,
    pub resolver: u64,
    pub offset: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub source: u64,
    pub template: Arc<Vec<u8>>,
    pub alignment: usize,
    pub descriptors: Vec<Descriptor>,
}
#[derive(Debug)]
struct Section {
    address: u64,
    length: usize,
    alignment: usize,
    kind: u32,
}
#[derive(Debug, Default)]
pub(crate) struct Sections {
    parts: Vec<Section>,
}

impl Sections {
    pub fn push(&mut self, section: &[u8], segment: (u64, u64, u64, u64, u32)) -> Result<()> {
        let (base, memory_size, file_size, file_offset, protection) = segment;
        let address = u64_at(section, 32)?;
        let length = u64_at(section, 40)?;
        let kind = u32_at(section, 64)? & 255;
        let exponent = u32_at(section, 52)?;
        if exponent > 12 || length > LIMIT as u64 || protection != 3 {
            return Err("unsupported TLS size/alignment/protection".into());
        }
        let alignment = 1u64 << exponent;
        let end = address.checked_add(length).ok_or("TLS section overflow")?;
        if address % alignment != 0
            || address < base
            || end
                > base
                    .checked_add(memory_size)
                    .ok_or("TLS segment overflow")?
        {
            return Err("invalid TLS section bounds/alignment".into());
        }
        if kind != 0x12
            && (end
                > base
                    .checked_add(file_size)
                    .ok_or("TLS file range overflow")?
                || u64::from(u32_at(section, 48)?)
                    != file_offset
                        .checked_add(address - base)
                        .ok_or("TLS file offset overflow")?)
        {
            return Err("TLS section not backed by matching file data".into());
        }
        if kind == 0x13
            && (address % 8 != 0 || length % 24 != 0 || length / 24 > 1024 || exponent < 3)
        {
            return Err("invalid TLS descriptor array".into());
        }
        if kind == 0x14
            && (address % 8 != 0 || length % 8 != 0 || length / 8 > 1024 || exponent < 3)
        {
            return Err("invalid TLS descriptor pointer array".into());
        }
        if self
            .parts
            .iter()
            .any(|s| s.kind == kind || (address < s.address + s.length as u64 && s.address < end))
        {
            return Err("duplicate/overlapping TLS sections".into());
        }
        self.parts.push(Section {
            address,
            length: length as usize,
            alignment: alignment as usize,
            kind,
        });
        Ok(())
    }
    pub fn contains_descriptor(&self, address: u64, slide: u64) -> bool {
        self.parts.iter().any(|s| {
            s.kind == 0x13
                && address
                    .checked_sub(slide)
                    .and_then(|a| a.checked_sub(s.address))
                    .is_some_and(|offset| offset < s.length as u64 && offset % 24 == 0)
        })
    }
    pub fn is_empty(&self) -> bool {
        self.parts.iter().all(|s| s.kind == 0x14)
    }
    pub fn validate_pointers(
        &self,
        image: &Image,
        slide: u64,
        targets: &std::collections::BTreeSet<u64>,
    ) -> Result<()> {
        for section in self.parts.iter().filter(|s| s.kind == 0x14) {
            let bytes = image.read(
                section
                    .address
                    .checked_add(slide)
                    .ok_or("TLS pointer slide overflow")?,
                section.length,
                1,
            )?;
            for offset in (0..section.length).step_by(8) {
                if !targets.contains(&u64_at(bytes, offset)?) {
                    return Err("TLS pointer slot does not name a registered descriptor".into());
                }
            }
        }
        Ok(())
    }
    pub fn finish(&self, object: &Object, slide: u64, resolver: u64) -> Result<Block> {
        let vars = self
            .parts
            .iter()
            .find(|s| s.kind == 0x13)
            .ok_or("TLS template without descriptors")?;
        let mut data: Vec<_> = self
            .parts
            .iter()
            .filter(|s| s.kind == 0x11 || s.kind == 0x12)
            .collect();
        data.sort_by_key(|s| s.address);
        let first = *data.first().ok_or("TLS descriptors without template")?;
        let mut end = first.address;
        for part in &data {
            if part.address != end || (part.kind == 0x11 && part.address != first.address) {
                return Err("noncontiguous/unsupported TLS template".into());
            }
            end = end
                .checked_add(part.length as u64)
                .ok_or("TLS template overflow")?;
            if part.kind == 0x12
                && object
                    .image
                    .read(
                        part.address
                            .checked_add(slide)
                            .ok_or("TLS slide overflow")?,
                        part.length,
                        1,
                    )?
                    .iter()
                    .any(|v| *v != 0)
            {
                return Err("nonzero TLS zerofill template".into());
            }
        }
        let source = first
            .address
            .checked_add(slide)
            .ok_or("TLS template slide overflow")?;
        let length =
            usize::try_from(end - first.address).map_err(|_| "TLS template size overflow")?;
        if length == 0 || length > LIMIT {
            return Err("TLS template exceeds cap".into());
        }
        let template = object.image.read(source, length, 3)?.to_vec().into();
        let mut descriptors = Vec::new();
        for offset in (0..vars.length).step_by(24) {
            let address = vars
                .address
                .checked_add(slide)
                .and_then(|v| v.checked_add(offset as u64))
                .ok_or("TLS descriptor slide overflow")?;
            let words = object.image.read(address, 24, 3)?;
            let offset = u64_at(words, 16)?;
            if u64_at(words, 0)? != resolver || u64_at(words, 8)? != 0 || offset >= length as u64 {
                return Err("unsupported TLS resolver/key/offset".into());
            }
            descriptors.push(Descriptor {
                address,
                resolver,
                offset,
            });
        }
        Ok(Block {
            source,
            template,
            alignment: data.iter().map(|s| s.alignment).max().unwrap(),
            descriptors,
        })
    }
}

/// Stable backing layout is fixed before native code captures buffer pointers.
pub(crate) fn layout(image: &Image) -> Result<(usize, Vec<u64>)> {
    for segment in &image.segments {
        segment
            .address
            .checked_add(segment.bytes.len() as u64)
            .ok_or("TLS image range overflow")?;
    }
    let mut length = 0usize;
    let mut bases = Vec::new();
    let mut addresses = std::collections::BTreeSet::new();
    for block in &image.tls {
        let alignment = block.alignment;
        if alignment == 0
            || !alignment.is_power_of_two()
            || alignment > 4096
            || block.template.is_empty()
            || block.template.len() > LIMIT
            || block.descriptors.is_empty()
        {
            return Err("invalid prepared TLS block".into());
        }
        if image.read(block.source, block.template.len(), 3)? != block.template.as_slice() {
            return Err("prepared TLS template mismatch".into());
        }
        length = length
            .checked_add(alignment - 1)
            .ok_or("TLS arena overflow")?
            & !(alignment - 1);
        bases.push(BASE + length as u64);
        length = length
            .checked_add(block.template.len())
            .ok_or("TLS arena overflow")?;
        if length > LIMIT {
            return Err("TLS storage exceeds 1 MiB cap".into());
        }
        for descriptor in &block.descriptors {
            if descriptor.address % 8 != 0
                || descriptor.offset >= block.template.len() as u64
                || !addresses.insert(descriptor.address)
                || addresses.len() > 1024
            {
                return Err("invalid/excessive prepared TLS descriptors".into());
            }
            let words = image.read(descriptor.address, 24, 3)?;
            if u64_at(words, 0)? != descriptor.resolver
                || u64_at(words, 8)? != 0
                || u64_at(words, 16)? != descriptor.offset
                || image.instruction(descriptor.resolver)? != SVC
                || image.instruction(
                    descriptor
                        .resolver
                        .checked_add(4)
                        .ok_or("TLS resolver overflow")?,
                )? != RET
            {
                return Err("invalid prepared TLS descriptor/resolver".into());
            }
        }
    }
    if length != 0 {
        let mapped = image.segments.iter().try_fold(0usize, |n, s| {
            n.checked_add(s.bytes.len())
                .ok_or("TLS mapped size overflow")
        })?;
        if mapped
            .checked_add(length)
            .ok_or("TLS mapped size overflow")?
            > 64 * 1024 * 1024
            || image.segments.iter().any(|s| {
                s.address < BASE + length as u64 && BASE < s.address + s.bytes.len() as u64
            })
        {
            return Err("TLS arena overlaps image or exceeds loaded memory cap".into());
        }
    }
    Ok((length, bases))
}
