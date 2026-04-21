//! Native representation of a compiled bytecode function, held inside
//! `RtData::BCFuncNative(Arc<BcFunc>)`. This replaces the nested RtValue tree
//! `(BCFunc name arity reg_count capture_count [consts] [[op dst a b] ...])`
//! — for a 100-instruction function, ~513 RtValue allocations collapse into a
//! single `Arc<BcFunc>`.
//!
//! Phase 1 of spec `2026-04-21-bcfunc-native-representation.md`: defines the
//! struct and `TAG_BCFUNC`. No AIRL-level use yet — the AIRL bootstrap
//! compiler continues to emit the legacy `(BCFunc ...)` variant, and
//! `value_to_bytecode_func` in airl-runtime keeps the old path. Later phases
//! switch construction and consumption to go through this type, then delete
//! the variant path.
//!
//! Design notes:
//!   * `Arc` rather than `Box` — a BCFunc can be referenced by closures, by
//!     the func_map, and by capture lists, all live at once. Arc keeps clones
//!     O(1) and drops when the last reference goes.
//!   * Fields use small-int types (`u16`, `u32`) to match
//!     `airl_runtime::bytecode::Instruction` layout exactly — conversion to
//!     the AOT path is a pointer copy, not a re-encoding.
//!   * `constants` uses `*mut RtValue` so that existing retain/release
//!     machinery in `memory.rs` handles lifetimes uniformly with lists/maps.

#[cfg(target_os = "airlos")]
#[allow(unused_imports)]
use crate::nostd_prelude::*;

use crate::value::RtValue;

/// One bytecode instruction. Layout mirrors `airl_runtime::bytecode::Instruction`
/// (`op` as a `u8` rather than the `Op` enum, to keep this crate free of a
/// dependency on the full opcode table). The runtime side casts the u8 back
/// to its `Op` enum at the boundary.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct BcInstr {
    pub op: u8,
    pub _pad: u8,
    pub dst: u16,
    pub a: u16,
    pub b: u16,
}

impl BcInstr {
    #[inline]
    pub fn new(op: u8, dst: u16, a: u16, b: u16) -> Self {
        Self { op, _pad: 0, dst, a, b }
    }
}

/// Opaque native form of a compiled function. See module docs for the
/// motivating size-vs-RtValue-tree tradeoff.
///
/// Constants are stored as `*mut RtValue` so retain/release goes through the
/// same code paths as list items. Cloning a `BcFunc` would double-retain
/// every constant, so don't `Clone` this directly — always share via
/// `Arc<BcFunc>` and only retain the constants once, at construction time.
#[derive(Debug)]
pub struct BcFunc {
    pub name: String,
    pub arity: u16,
    pub reg_count: u16,
    pub capture_count: u16,
    pub constants: Vec<*mut RtValue>,
    pub instructions: Vec<BcInstr>,
}

impl BcFunc {
    pub fn new(name: String, arity: u16, reg_count: u16, capture_count: u16) -> Self {
        Self {
            name,
            arity,
            reg_count,
            capture_count,
            constants: Vec::new(),
            instructions: Vec::new(),
        }
    }

    /// Retains `v` and appends it as a new constant. Returns the constant's
    /// index (0-based). Used by `bc-func-push-const`.
    pub fn push_const(&mut self, v: *mut RtValue) -> usize {
        crate::memory::airl_value_retain(v);
        let idx = self.constants.len();
        self.constants.push(v);
        idx
    }

    pub fn push_instr(&mut self, instr: BcInstr) {
        self.instructions.push(instr);
    }
}

impl Drop for BcFunc {
    fn drop(&mut self) {
        // Release the +1 retain that push_const did. If a BcFunc was
        // constructed directly via struct literal, callers are responsible
        // for the retain balance on `constants` — mirror the list/map
        // convention where `rt_list(vec![p])` expects `p` to already carry
        // the reference we're taking ownership of.
        for p in self.constants.drain(..) {
            crate::memory::airl_value_release(p);
        }
    }
}

// SAFETY: BcFunc's `Vec<*mut RtValue>` pointers own one reference each.
// Ownership is exclusive — the Arc<BcFunc> layer provides concurrent readers,
// never mutators — so cross-thread access is safe as long as any mutation
// happens behind Arc::get_mut / Arc::make_mut (both already guarantee
// exclusive access). This matches the Send/Sync bounds airl-rt gives
// RtValue itself.
unsafe impl Send for BcFunc {}
unsafe impl Sync for BcFunc {}
