#![cfg_attr(target_os = "airlos", no_std)]

#[cfg(target_os = "airlos")]
#[macro_use]
extern crate alloc;

#[cfg(not(target_os = "airlos"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(target_os = "airlos")]
pub mod airlos;

// ── AIRLOS global allocator ──────────────────────────────────────────
// First-fit allocator with splitting and coalescing, backed by SYS_BRK.
// Ported from AIRLOS rt_alloc.c to avoid cross-language linking.
#[cfg(target_os = "airlos")]
mod airlos_alloc {
    use core::alloc::{GlobalAlloc, Layout};
    use core::sync::atomic::{AtomicUsize, Ordering::Relaxed};

    /// Block header: 16 bytes on x86-64, guarantees 16-byte payload alignment.
    /// Layout: [size: u64 | flags: u64] [payload ...]
    const HEADER_SIZE: usize = 16;
    const MIN_BLOCK: usize = 16;
    const GROW_ALIGN: usize = 4096; // grow heap in page increments

    const SYS_BRK: u32 = 33;

    struct AirlosAlloc;

    static HEAP_START: AtomicUsize = AtomicUsize::new(0);
    static HEAP_END: AtomicUsize = AtomicUsize::new(0);

    unsafe fn sys_brk(addr: usize) -> usize {
        let ret: usize;
        core::arch::asm!(
            "xchg rbx, {arg}",
            "int 0x80",
            "xchg rbx, {arg}",
            arg = inout(reg) addr => _,
            inout("eax") SYS_BRK as usize => ret,
            options(nostack),
        );
        ret
    }

    /// Read the size field from a block header at `ptr`.
    unsafe fn hdr_size(ptr: usize) -> usize {
        *(ptr as *const usize)
    }

    /// Read the flags field from a block header at `ptr`.
    unsafe fn hdr_flags(ptr: usize) -> usize {
        *((ptr + 8) as *const usize)
    }

    /// Write size and flags to a block header at `ptr`.
    unsafe fn hdr_set(ptr: usize, size: usize, flags: usize) {
        *(ptr as *mut usize) = size;
        *((ptr + 8) as *mut usize) = flags;
    }

    unsafe fn grow_heap(needed: usize) -> bool {
        let heap_end = HEAP_END.load(Relaxed);
        let grow = (needed + GROW_ALIGN - 1) & !(GROW_ALIGN - 1);
        let new_end = heap_end + grow;
        let result = sys_brk(new_end);
        if result == 0 {
            return false;
        }
        HEAP_END.store(new_end, Relaxed);
        true
    }

    unsafe fn ensure_init() -> bool {
        if HEAP_START.load(Relaxed) != 0 {
            return true;
        }
        let brk = sys_brk(0);
        if brk == 0 {
            return false;
        }
        HEAP_START.store(brk, Relaxed);
        HEAP_END.store(brk, Relaxed);
        true
    }

    unsafe impl GlobalAlloc for AirlosAlloc {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if !ensure_init() {
                return core::ptr::null_mut();
            }

            // Round size up to 16-byte boundary for alignment
            let size = (layout.size().max(1) + 15) & !15;

            let heap_start = HEAP_START.load(Relaxed);
            let heap_end = HEAP_END.load(Relaxed);

            // Walk block list looking for a free block (first-fit)
            let mut ptr = heap_start;
            while ptr + HEADER_SIZE <= heap_end {
                let bsize = hdr_size(ptr);
                if bsize == 0 {
                    break; // end of used region
                }
                let bflags = hdr_flags(ptr);

                if (bflags & 1) != 0 && bsize >= size {
                    // Found a free block — split if large enough
                    if bsize >= size + HEADER_SIZE + MIN_BLOCK {
                        let next = ptr + HEADER_SIZE + size;
                        hdr_set(next, bsize - size - HEADER_SIZE, 1); // free remainder
                        hdr_set(ptr, size, 0); // used
                    } else {
                        hdr_set(ptr, bsize, 0); // use whole block
                    }
                    return (ptr + HEADER_SIZE) as *mut u8;
                }

                ptr += HEADER_SIZE + bsize;
            }

            // No free block found — grow heap and bump
            let total = HEADER_SIZE + size;
            if ptr + total > heap_end {
                if !grow_heap(ptr + total - heap_end) {
                    crate::airlos::serial_print("ALLOC: OOM\n");
                    return core::ptr::null_mut();
                }
            }

            hdr_set(ptr, size, 0); // used
            (ptr + HEADER_SIZE) as *mut u8
        }

        unsafe fn dealloc(&self, p: *mut u8, _layout: Layout) {
            if p.is_null() {
                return;
            }
            let hdr = (p as usize) - HEADER_SIZE;
            let size = hdr_size(hdr);

            // Mark free
            hdr_set(hdr, size, 1);

            // Coalesce with next block if also free
            let next = hdr + HEADER_SIZE + size;
            let heap_end = HEAP_END.load(Relaxed);
            if next + HEADER_SIZE <= heap_end {
                let next_size = hdr_size(next);
                if next_size > 0 && (hdr_flags(next) & 1) != 0 {
                    hdr_set(hdr, size + HEADER_SIZE + next_size, 1);
                }
            }
        }
    }

    #[global_allocator]
    static ALLOC: AirlosAlloc = AirlosAlloc;
}

// ── AIRLOS panic handler ─────────────────────────────────────────────
#[cfg(target_os = "airlos")]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // vga_print already echoes to serial, so this covers both outputs.
    // Safe to call here: vga_print uses only stack buffers + syscalls, no heap.
    fn emit(s: &str) {
        crate::airlos::vga_print(s);
    }
    emit("PANIC: ");
    if let Some(loc) = info.location() {
        emit(loc.file());
        emit(":");
        // Print line number as digits
        let line = loc.line();
        let mut buf = [0u8; 10];
        let mut n = line;
        let mut i = 9;
        loop {
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
            if n == 0 { break; }
            if i == 0 { break; }
            i -= 1;
        }
        if let Ok(s) = core::str::from_utf8(&buf[i..10]) {
            emit(s);
        }
    }
    emit("\n");
    crate::airlos::exit(1);
}

// ── no_std prelude: re-export alloc types for all modules ────────────
#[cfg(target_os = "airlos")]
pub(crate) mod nostd_prelude {
    pub use alloc::boxed::Box;
    pub use alloc::string::{String, ToString};
    pub use alloc::vec::Vec;
}

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
pub mod identity;
#[cfg(not(target_os = "airlos"))]
pub mod thread;
#[cfg(not(target_os = "airlos"))]
pub mod terminal;
