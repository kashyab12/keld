mod chained;
pub mod engine;
pub mod isa;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub mod jit;
pub mod linker;
pub mod macho;
pub mod memory;

pub type Result<T> = std::result::Result<T, String>;
