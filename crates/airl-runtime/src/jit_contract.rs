//! Shared contract-error signaling for JIT-compiled code.
//!
//! Both the full JIT (`bytecode_jit_full`) and AOT paths call
//! `airl_jit_contract_fail` from native code when a contract assertion
//! fails. The error is stored in a thread-local cell and retrieved by
//! the bytecode VM after the native call returns.

use std::cell::RefCell;

thread_local! {
    static JIT_CONTRACT_ERROR: RefCell<Option<(u8, u16, u16)>> = const { RefCell::new(None) };
}

/// C-ABI function called by JIT-compiled code when a contract assertion fails.
/// Stores error info in thread-local cell and returns a sentinel value.
/// kind: 0=Requires, 1=Ensures, 2=Invariant
#[no_mangle]
pub extern "C" fn airl_jit_contract_fail(kind: u64, fn_name_idx: u64, clause_idx: u64) -> u64 {
    JIT_CONTRACT_ERROR.with(|cell| {
        *cell.borrow_mut() = Some((kind as u8, fn_name_idx as u16, clause_idx as u16));
    });
    u64::MAX // sentinel — VM checks error cell when it sees this
}

/// Check if a JIT contract error was signaled and extract it.
pub fn take_jit_contract_error() -> Option<(u8, u16, u16)> {
    JIT_CONTRACT_ERROR.with(|cell| cell.borrow_mut().take())
}
