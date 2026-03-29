use std::cell::RefCell;
use std::mem::MaybeUninit;

use crate::value::RtValue;

const INITIAL_SLAB_SIZE: usize = 1024;
const MAX_SLAB_SIZE: usize = 8192;

/// Pre-allocated slab of RtValue slots.
/// Slabs are heap-allocated and intentionally leaked on thread exit
/// to prevent dangling pointers from cross-thread refcount releases.
struct Slab {
    data: Box<[MaybeUninit<RtValue>]>,
}

pub struct ValuePool {
    slabs: Vec<Slab>,
    free_list: Vec<*mut RtValue>,
    next_slab_size: usize,
}

impl ValuePool {
    pub fn new() -> Self {
        ValuePool {
            slabs: Vec::new(),
            free_list: Vec::new(),
            next_slab_size: INITIAL_SLAB_SIZE,
        }
    }

    pub fn alloc(&mut self) -> *mut RtValue {
        self.free_list.pop().unwrap_or_else(|| self.grow())
    }

    pub fn release(&mut self, ptr: *mut RtValue) {
        self.free_list.push(ptr);
    }

    fn grow(&mut self) -> *mut RtValue {
        let size = self.next_slab_size;
        let mut slab_data: Vec<MaybeUninit<RtValue>> = Vec::with_capacity(size);
        unsafe { slab_data.set_len(size); }
        let mut slab = Slab { data: slab_data.into_boxed_slice() };
        // Push all slots except the last onto free list; return the last directly
        for i in 0..size - 1 {
            self.free_list.push(slab.data[i].as_mut_ptr());
        }
        let result = slab.data[size - 1].as_mut_ptr();
        self.slabs.push(slab);
        self.next_slab_size = (size * 2).min(MAX_SLAB_SIZE);
        result
    }
}

impl Drop for ValuePool {
    fn drop(&mut self) {
        // Leak all slabs to prevent dangling pointers.
        // Cross-thread values may still hold pointers into these slabs.
        // AIRL runs to completion; this memory is reclaimed by the OS at exit.
        for slab in self.slabs.drain(..) {
            std::mem::forget(slab);
        }
    }
}

thread_local! {
    static POOL: RefCell<ValuePool> = RefCell::new(ValuePool::new());
}

/// Allocate an RtValue slot from the thread-local pool.
/// Caller must write a valid RtValue to the returned pointer.
pub fn pool_alloc() -> *mut RtValue {
    POOL.with(|p| p.borrow_mut().alloc())
}

/// Return an RtValue slot to the thread-local pool.
/// Caller must have already dropped the RtValue's data via drop_in_place.
pub fn pool_release(ptr: *mut RtValue) {
    POOL.with(|p| p.borrow_mut().release(ptr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_alloc_release_reuse() {
        let mut pool = ValuePool::new();
        let p1 = pool.alloc();
        let p2 = pool.alloc();
        assert_ne!(p1, p2);
        pool.release(p1);
        let p3 = pool.alloc();
        assert_eq!(p1, p3); // reused
    }

    #[test]
    fn pool_grows() {
        let mut pool = ValuePool::new();
        // Allocate more than initial slab
        let mut ptrs = Vec::new();
        for _ in 0..INITIAL_SLAB_SIZE + 1 {
            ptrs.push(pool.alloc());
        }
        assert!(pool.slabs.len() >= 2);
        for p in ptrs {
            pool.release(p);
        }
    }

    #[test]
    fn thread_local_pool_works() {
        let p1 = pool_alloc();
        let p2 = pool_alloc();
        assert_ne!(p1, p2);
        pool_release(p1);
        pool_release(p2);
    }
}
