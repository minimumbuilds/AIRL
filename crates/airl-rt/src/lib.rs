#[cfg(not(target_os = "airlos"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(target_os = "airlos")]
pub mod airlos;

pub mod value;
pub mod memory;
pub mod error;
pub mod arithmetic;
pub mod comparison;
pub mod logic;
pub mod list;
pub mod string;
pub mod map;
pub mod io;
pub mod math;
pub mod variant;
pub mod closure;
pub mod misc;
#[cfg(not(target_os = "airlos"))]
pub mod thread;
