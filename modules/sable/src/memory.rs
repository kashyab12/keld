//! Per-run writable memory. Executable image bytes never become writable.
use crate::{
    Result,
    macho::{Image, Segment},
};

pub const STACK_BASE: u64 = 0x7000_0000;
pub const STACK_SIZE: usize = 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub struct Memory {
    pub(crate) regions: Vec<Segment>,
}

impl Memory {
    pub fn new(image: &Image) -> Result<Self> {
        let mut regions = image.segments.clone();
        if regions.iter().any(|s| {
            s.address < STACK_BASE + STACK_SIZE as u64
                && STACK_BASE < s.address + s.bytes.len() as u64
        }) {
            return Err("image overlaps reserved guest stack".into());
        }
        regions.push(Segment {
            address: STACK_BASE,
            bytes: vec![0; STACK_SIZE],
            protection: 3,
        });
        Ok(Self { regions })
    }

    fn locate(&self, address: u64, length: usize, permission: u32) -> Result<(usize, usize)> {
        for (index, region) in self.regions.iter().enumerate() {
            if region.protection & permission != permission {
                continue;
            }
            if let Some(offset) = address
                .checked_sub(region.address)
                .and_then(|n| usize::try_from(n).ok())
                && let Some(end) = offset.checked_add(length)
                && end <= region.bytes.len()
            {
                return Ok((index, offset));
            }
        }
        Err(format!(
            "guest memory fault at {address:#x}, length {length}, permission {permission}"
        ))
    }

    pub fn read(&self, address: u64, length: usize) -> Result<&[u8]> {
        let (index, offset) = self.locate(address, length, 1)?;
        Ok(&self.regions[index].bytes[offset..offset + length])
    }

    pub fn write(&mut self, address: u64, bytes: &[u8]) -> Result<()> {
        let (index, offset) = self.locate(address, bytes.len(), 2)?;
        self.regions[index].bytes[offset..offset + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    pub fn layout(&self) -> Vec<(u64, usize, u32)> {
        self.regions
            .iter()
            .map(|s| (s.address, s.bytes.len(), s.protection))
            .collect()
    }
}
