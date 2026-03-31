# Spec: Distinguish "Channel Closed" from "Invalid Handle"

**Date:** 2026-03-31
**Severity:** Low
**Found by:** TLA+ model checker (tla/airl_channels.tla, NoSendAfterClose invariant)

## Bug

After calling `(channel-close tx)`, subsequent `(channel-send tx value)` returns `(Err "invalid sender handle 5")` instead of `(Err "channel closed")`. The error is indistinguishable from passing a wrong handle number.

### Reproduction (TLC counterexample)

```
Step 1: Create channel → [tx=1, rx=2]
Step 2: (channel-close tx)  → sender removed from HashMap
Step 3: (channel-send tx 42) → senders.get(1) returns None → "invalid sender handle 1"
```

### Root Cause

`crates/airl-rt/src/thread.rs` lines 156-169:

```rust
let senders = channel_senders().lock().unwrap();
match senders.get(&tx_id) {
    Some(tx) => match tx.send(SendPtr(value)) {
        Ok(()) => rt_ok(rt_bool(true)),
        Err(_) => {
            crate::memory::airl_value_release(value);
            rt_err("channel closed")  // only reached if Receiver was dropped
        }
    },
    None => {
        crate::memory::airl_value_release(value);
        rt_err(&format!("channel-send: invalid sender handle {}", tx_id))
        // ^^^ also reached when handle was explicitly closed
    }
}
```

The `None` branch handles two different cases identically:
1. Handle ID was never valid (typo, wrong variable)
2. Handle was valid but explicitly closed via `channel-close`

The same issue exists in `channel-recv` — after closing the receiver, recv returns "invalid receiver handle" not "channel closed".

## Fix

Track closed handles in a separate set. Check it before the active map.

### Changes Required

**File:** `crates/airl-rt/src/thread.rs`

#### 1. Add closed handle tracking

```rust
fn closed_handles() -> &'static Mutex<HashSet<i64>> {
    static CLOSED: OnceLock<Mutex<HashSet<i64>>> = OnceLock::new();
    CLOSED.get_or_init(|| Mutex::new(HashSet::new()))
}
```

#### 2. Update channel_close to record the handle

```rust
pub extern "C" fn airl_channel_close(handle: *mut RtValue) -> *mut RtValue {
    let handle_id = match extract_int(handle) {
        Some(n) => n,
        None => return rt_bool(false),
    };
    let removed_tx = channel_senders().lock().unwrap().remove(&handle_id).is_some();
    let removed_rx = channel_receivers().lock().unwrap().remove(&handle_id).is_some();
    if removed_tx || removed_rx {
        closed_handles().lock().unwrap().insert(handle_id);
    }
    rt_bool(removed_tx || removed_rx)
}
```

#### 3. Update channel_send to check closed set

```rust
pub extern "C" fn airl_channel_send(tx_handle: *mut RtValue, value: *mut RtValue) -> *mut RtValue {
    let tx_id = match extract_int(tx_handle) {
        Some(n) => n,
        None => return rt_err("channel-send: handle must be Int"),
    };

    if closed_handles().lock().unwrap().contains(&tx_id) {
        airl_value_retain(value); // retain was already called by caller convention
        crate::memory::airl_value_release(value);
        return rt_err("channel-send: channel closed");
    }

    airl_value_retain(value);
    let senders = channel_senders().lock().unwrap();
    match senders.get(&tx_id) {
        Some(tx) => match tx.send(SendPtr(value)) {
            Ok(()) => rt_ok(rt_bool(true)),
            Err(_) => {
                crate::memory::airl_value_release(value);
                rt_err("channel-send: channel closed")
            }
        },
        None => {
            crate::memory::airl_value_release(value);
            rt_err(&format!("channel-send: invalid sender handle {}", tx_id))
        }
    }
}
```

Apply the same pattern to `channel-recv`.

## Testing

### Fixture test

`tests/fixtures/valid/channel_close_errors.airl`:
```clojure
;; Verify error messages distinguish closed vs invalid handles
(let (ch : List (channel-new))
     (tx : i64 (at ch 0))
  (do
    (channel-close tx)
    (let (r : _ (channel-send tx 42))
      (match r
        (Ok _) "FAIL:should-error"
        (Err e) (if (contains e "closed") "ok" (str "FAIL:" e))))))
```

Expected: `"ok"` (currently produces `"FAIL:invalid sender handle"`)
