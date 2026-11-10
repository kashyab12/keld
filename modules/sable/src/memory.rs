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
        for region in &mut regions {
            if region.protection & 2 != 0 {
                region.bytes = region.bytes.as_ref().clone().into();
            }
        }
        if regions.iter().any(|s| {
            s.address < STACK_BASE + STACK_SIZE as u64
                && STACK_BASE < s.address + s.bytes.len() as u64
        }) {
            return Err("image overlaps reserved guest stack".into());
        }
        regions.push(Segment {
            address: STACK_BASE,
            bytes: vec![0; STACK_SIZE].into(),
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
        let target = std::sync::Arc::get_mut(&mut self.regions[index].bytes)
            .ok_or("writable guest region unexpectedly aliased")?;
        target[offset..offset + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    /// Immutable image bytes are shared; writable bytes and stack are always private.
    pub fn shared_image_bytes(&self) -> usize {
        self.regions
            .iter()
            .filter(|r| r.protection & 2 == 0)
            .map(|r| r.bytes.len())
            .sum()
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    pub(crate) fn pointers(&mut self) -> Result<Vec<*mut u8>> {
        self.regions
            .iter_mut()
            .map(|r| {
                if r.protection & 2 != 0 {
                    Ok(std::sync::Arc::get_mut(&mut r.bytes)
                        .ok_or("writable guest region unexpectedly aliased")?
                        .as_mut_ptr())
                } else {
                    Ok(r.bytes.as_ptr().cast_mut())
                }
            })
            .collect()
    }

    pub fn layout(&self) -> Vec<(u64, usize, u32)> {
        self.regions
            .iter()
            .map(|s| (s.address, s.bytes.len(), s.protection))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn immutable_bytes_are_shared_and_writable_bytes_are_private() {
        let image = Image {
            segments: vec![
                Segment {
                    address: 0x1000,
                    bytes: vec![1; 16].into(),
                    protection: 5,
                },
                Segment {
                    address: 0x2000,
                    bytes: vec![2; 16].into(),
                    protection: 3,
                },
            ],
            entry: 0x1000,
            text: (0x1000, 16),
            extra_text: Vec::new(),
            platform: 7,
        };
        let mut a = Memory::new(&image).unwrap();
        let b = Memory::new(&image).unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &a.regions[0].bytes,
            &image.segments[0].bytes
        ));
        assert!(!std::sync::Arc::ptr_eq(
            &a.regions[1].bytes,
            &b.regions[1].bytes
        ));
        a.write(0x2000, &[9]).unwrap();
        assert_eq!(b.read(0x2000, 1).unwrap(), [2]);
        assert_eq!(image.read(0x2000, 1, 1).unwrap(), [2]);
        assert!(a.write(0x1000, &[9]).is_err());
        assert_eq!(a.shared_image_bytes(), 16);
    }
}
