//! Native representation of a compiled bytecode function, held inside
//! `RtData::BCFuncNative(Arc<BcFunc>)`. This replaces the nested RtValue tree
//! `(BCFunc name arity reg_count capture_count [consts] [[op dst a b] ...])`
//! — for a 100-instruction function, ~513 RtValue allocations collapse into a
//! single `Arc<BcFunc>`.
//!
//! Spec `2026-04-21-bcfunc-native-representation.md`. Phase 1 introduced the
//! struct and `TAG_BCFUNC`; Phase 2 added `bc-func-from` and migrated
//! `bootstrap/bc_compiler.airl` to use it; Phase 4 deleted the legacy
//! `(BCFunc ...)` nested-variant path entirely. All bytecode functions now
//! flow exclusively through this type.
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

// ── Builtins exposed to AIRL as (bc-func-* ...) ────────────────────────
//
// Phase 2 of spec 2026-04-21-bcfunc-native-representation.md. These are
// the minimal builtins needed for bootstrap/bc_compiler.airl to emit the
// native form and for bootstrap/g3_compiler.airl to consume it.
//
// `bc-func-from` is the single-shot constructor — takes the six fields as
// separate arguments and builds an `Arc<BcFunc>` wrapped in an RtValue.
// `bc-func-is-main?` replaces the common pattern-match site
//   `(match f (BCFunc name _ _ _ _ _) (= name "__main__"))`.

use crate::value::{rt_bcfunc, rt_bool, RtData};
#[cfg(not(target_os = "airlos"))]
use std::sync::Arc;

/// `(bc-func-from name arity reg_count capture_count constants instructions)`
/// Returns a BCFuncNative RtValue. Instructions are a flat list of
/// `[op dst a b]` lists (legacy shape emitted by bc_compiler.airl).
#[no_mangle]
pub extern "C" fn airl_bc_func_from(
    name: *mut RtValue,
    arity: *mut RtValue,
    reg_count: *mut RtValue,
    capture_count: *mut RtValue,
    constants: *mut RtValue,
    instructions: *mut RtValue,
) -> *mut RtValue {
    let name_str = unsafe { &*name }.as_str_owned();
    let arity_v = unsafe { &*arity }.as_int() as u16;
    let reg_count_v = unsafe { &*reg_count }.as_int() as u16;
    let capture_count_v = unsafe { &*capture_count }.as_int() as u16;

    let const_slice_vec: Vec<*mut RtValue> = crate::list::list_items(&unsafe { &*constants }.data)
        .iter()
        .copied()
        .collect();
    let mut consts_vec: Vec<*mut RtValue> = Vec::with_capacity(const_slice_vec.len());
    for p in &const_slice_vec {
        crate::memory::airl_value_retain(*p);
        consts_vec.push(*p);
    }

    let instr_slice_vec: Vec<*mut RtValue> = crate::list::list_items(&unsafe { &*instructions }.data)
        .iter()
        .copied()
        .collect();
    let mut instrs_vec: Vec<BcInstr> = Vec::with_capacity(instr_slice_vec.len());
    for inst_ptr in &instr_slice_vec {
        let inst_slice = crate::list::list_items(&unsafe { &**inst_ptr }.data);
        if inst_slice.len() < 4 {
            crate::error::rt_error("bc-func-from: each instruction must be a [op dst a b] list");
        }
        let op = unsafe { &*inst_slice[0] }.as_int() as u8;
        let dst = unsafe { &*inst_slice[1] }.as_int() as u16;
        let a = unsafe { &*inst_slice[2] }.as_int() as u16;
        let b = unsafe { &*inst_slice[3] }.as_int() as u16;
        instrs_vec.push(BcInstr::new(op, dst, a, b));
    }

    let bcf = Arc::new(BcFunc {
        name: name_str,
        arity: arity_v,
        reg_count: reg_count_v,
        capture_count: capture_count_v,
        constants: consts_vec,
        instructions: instrs_vec,
    });
    rt_bcfunc(bcf)
}

/// `(bc-func-is-main? bcf) -> Bool` — true when the function's name is
/// `__main__`. `bc_compiler.airl` only emits `BCFuncNative` via the
/// `bc-func-from` builtin; the legacy `(BCFunc ...)` variant path was
/// removed in spec 3 phase 4.
#[no_mangle]
pub extern "C" fn airl_bc_func_is_main(bcf: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &*bcf };
    match &v.data {
        RtData::BCFuncNative(b) => rt_bool(b.name == "__main__"),
        _ => rt_bool(false),
    }
}

/// `(bc-func-name bcf) -> String`. Diagnostic accessor.
#[no_mangle]
pub extern "C" fn airl_bc_func_name(bcf: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &*bcf };
    match &v.data {
        RtData::BCFuncNative(b) => crate::value::rt_str(b.name.clone()),
        _ => crate::error::rt_error("bc-func-name: not a BCFuncNative"),
    }
}
