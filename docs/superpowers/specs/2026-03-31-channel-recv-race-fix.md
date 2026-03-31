# Spec: Fix Concurrent Channel Recv Race

**Date:** 2026-03-31
**Severity:** High
**Found by:** TLA+ model checker (tla/airl_channels.tla, NoConcurrentRecvConflict invariant)

## Bug

When two threads share the same receiver handle and both call `channel-recv`, the second caller gets "invalid receiver handle" instead of blocking or receiving a message.

### Reproduction (TLC counterexample)

```
Step 1: Thread 1 calls (channel-recv rx) → queue empty → blocks
Step 2: Thread 2 calls (channel-recv rx) → "invalid receiver handle"
```

### Root Cause

`crates/airl-rt/src/thread.rs` lines 180-188:

```rust
let rx = channel_receivers().lock().unwrap().remove(&rx_id);  // REMOVES from map
match rx {
    Some(rx) => {
        let result = match rx.recv() { ... };  // blocks with rx removed
        channel_receivers().lock().unwrap().insert(rx_id, rx);  // re-inserts AFTER
        result
    }
    None => rt_err("invalid receiver handle"),  // second caller hits this
}
```

The `remove()` → `recv()` → `insert()` pattern is not atomic. The receiver is absent from the global map for the entire duration of the blocking `recv()` call. Any other thread calling `channel-recv` or `channel-drain` on the same handle during this window gets a spurious "invalid handle" error.

The same pattern exists in `airl_channel_recv_timeout` (lines 206-219) and `airl_channel_drain` (lines 231-245).

### Impact

- Any AIRL program with a work-stealing pattern (multiple consumer threads on one channel) is broken
- The error message is indistinguishable from a genuine invalid handle, making debugging difficult
- `channel-drain` has the same bug — concurrent drain + recv will fail

## Fix

Replace the per-handle `Receiver<SendPtr>` storage with `Arc<Mutex<Receiver<SendPtr>>>`. Multiple threads can then lock the mutex to access the receiver without removing it from the map.

### Changes Required

**File:** `crates/airl-rt/src/thread.rs`

#### 1. Change receiver storage type

```rust
// Before:
fn channel_receivers() -> &'static Mutex<HashMap<i64, std::sync::mpsc::Receiver<SendPtr>>> {

// After:
use std::sync::Arc;
type SharedReceiver = Arc<Mutex<std::sync::mpsc::Receiver<SendPtr>>>;

fn channel_receivers() -> &'static Mutex<HashMap<i64, SharedReceiver>> {
```

#### 2. Update channel_new

```rust
pub extern "C" fn airl_channel_new() -> *mut RtValue {
    let (tx, rx) = std::sync::mpsc::channel();
    let tx_id = NEXT_CHANNEL_HANDLE.fetch_add(1, Ordering::SeqCst);
    let rx_id = NEXT_CHANNEL_HANDLE.fetch_add(1, Ordering::SeqCst);
    channel_senders().lock().unwrap().insert(tx_id, tx);
    channel_receivers().lock().unwrap().insert(rx_id, Arc::new(Mutex::new(rx)));
    // ...
}
```

#### 3. Update channel_recv (and recv_timeout, drain)

```rust
pub extern "C" fn airl_channel_recv(rx_handle: *mut RtValue) -> *mut RtValue {
    let rx_id = match extract_int(rx_handle) {
        Some(n) => n,
        None => return rt_err("channel-recv: handle must be Int"),
    };

    // Clone the Arc — does NOT remove from map
    let rx_arc = match channel_receivers().lock().unwrap().get(&rx_id) {
        Some(arc) => arc.clone(),
        None => return rt_err(&format!("channel-recv: invalid receiver handle {}", rx_id)),
    };

    // Lock the receiver — blocks other recv callers (they wait, not error)
    let rx = rx_arc.lock().unwrap();
    match rx.recv() {
        Ok(SendPtr(val)) => rt_ok(val),
        Err(_) => rt_err("channel closed"),
    }
}
```

Apply the same pattern to `airl_channel_recv_timeout` and `airl_channel_drain`.

#### 4. Update channel_close

```rust
pub extern "C" fn airl_channel_close(handle: *mut RtValue) -> *mut RtValue {
    let handle_id = match extract_int(handle) {
        Some(n) => n,
        None => return rt_bool(false),
    };
    // Removing the Arc from the map. If other threads hold clones,
    // they can finish their current recv but new lookups will fail.
    let removed_tx = channel_senders().lock().unwrap().remove(&handle_id).is_some();
    let removed_rx = channel_receivers().lock().unwrap().remove(&handle_id).is_some();
    rt_bool(removed_tx || removed_rx)
}
```

## Testing

### Unit test in thread.rs

```rust
#[test]
fn concurrent_recv_on_shared_channel() {
    let new_result = airl_channel_new();
    // extract tx_id, rx_id from list
    // Spawn two threads that both call airl_channel_recv on the same rx_id
    // Send two messages
    // Both threads should receive one message each (not "invalid handle")
}
```

### Fixture test

`tests/fixtures/valid/channel_concurrent_recv.airl`:
```clojure
;; Two consumer threads on one channel — both should get a message
(let (ch : List (channel-new))
     (tx : i64 (at ch 0))
     (rx : i64 (at ch 1))
     (t1 : i64 (thread-spawn (fn [] (channel-recv rx))))
     (t2 : i64 (thread-spawn (fn [] (channel-recv rx))))
  (do
    (channel-send tx 1)
    (channel-send tx 2)
    (let (r1 : _ (thread-join t1))
         (r2 : _ (thread-join t2))
      (str (match r1 (Ok v) "ok" (Err e) e) "|"
           (match r2 (Ok v) "ok" (Err e) e)))))
```

Expected: `"ok|ok"` (currently produces `"ok|invalid receiver handle"`)

## Verification

After fix, re-run TLC:
```bash
cd tla && java -jar tla2tools.jar -config MC_channels.cfg airl_channels.tla
```

`NoConcurrentRecvConflict` invariant should pass.
