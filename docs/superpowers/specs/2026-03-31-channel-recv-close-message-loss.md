# Spec: Handle Message Loss on Receiver Close

**Date:** 2026-03-31
**Severity:** Medium
**Found by:** TLA+ model checker (tla/airl_channels.tla, NoMessagesLost invariant)

## Bug

Closing a channel's receiver handle silently discards all messages buffered in the channel. The sender has no way to know its messages were dropped.

### Reproduction (TLC counterexample)

```
Step 1: Create channel → [tx=1, rx=2]
Step 2: (channel-send tx "important data") → queued in buffer
Step 3: (channel-close rx) → Receiver dropped, buffer dropped, message gone
```

No error is returned to anyone. The sender's `channel-send` already returned `(Ok true)`. The message simply vanishes.

### Root Cause

`crates/airl-rt/src/thread.rs` lines 250-258:

```rust
pub extern "C" fn airl_channel_close(handle: *mut RtValue) -> *mut RtValue {
    let handle_id = match extract_int(handle) {
        Some(n) => n,
        None => return rt_bool(false),
    };
    let removed_tx = channel_senders().lock().unwrap().remove(&handle_id).is_some();
    let removed_rx = channel_receivers().lock().unwrap().remove(&handle_id).is_some();
    rt_bool(removed_tx || removed_rx)
}
```

When `remove()` drops the `Receiver<SendPtr>`, Rust drops the internal buffer, which contains `SendPtr(*mut RtValue)` wrappers. These wrappers do NOT call `airl_value_release` on the contained `RtValue` pointers — they simply deallocate the `SendPtr` struct. This means:

1. **Messages are silently lost** — no delivery, no error, no notification
2. **Memory is leaked** — the `RtValue` objects inside the messages are never released (their refcount is never decremented)

### Impact

- Programs that close channels before draining them leak memory
- Producer-consumer patterns where the consumer exits early lose in-flight work
- No way to implement graceful shutdown (drain then close) because there's no "close after drain" primitive

## Fix

Drain and release all buffered messages before dropping the receiver.

### Changes Required

**File:** `crates/airl-rt/src/thread.rs`

#### Update channel_close to drain before dropping

```rust
pub extern "C" fn airl_channel_close(handle: *mut RtValue) -> *mut RtValue {
    let handle_id = match extract_int(handle) {
        Some(n) => n,
        None => return rt_bool(false),
    };

    let removed_tx = channel_senders().lock().unwrap().remove(&handle_id).is_some();

    // If closing a receiver, drain and release all buffered messages first
    let removed_rx = if let Some(rx) = channel_receivers().lock().unwrap().remove(&handle_id) {
        // For Arc<Mutex<Receiver>> (after recv race fix):
        // let rx = rx.lock().unwrap();
        loop {
            match rx.try_recv() {
                Ok(SendPtr(val)) => crate::memory::airl_value_release(val),
                Err(_) => break,
            }
        }
        true
    } else {
        false
    };

    rt_bool(removed_tx || removed_rx)
}
```

This ensures:
1. All buffered `RtValue` objects have their refcount decremented
2. No memory leak from dropped messages
3. The close operation is explicit about discarding remaining messages

### Alternative: Return dropped count

For programs that need to know messages were lost:

```rust
// Return the number of messages that were in the buffer when closed
// (0 if closing a sender, N if closing a receiver with N buffered messages)
pub extern "C" fn airl_channel_close(handle: *mut RtValue) -> *mut RtValue {
    // ... same drain logic ...
    rt_int(dropped_count as i64)  // instead of rt_bool
}
```

This is a breaking change to the return type. The boolean return is simpler and sufficient.

## Testing

### Fixture test

`tests/fixtures/valid/channel_close_drain.airl`:
```clojure
;; Verify closing receiver doesn't leak (test by checking it doesn't crash)
(let (ch : List (channel-new))
     (tx : i64 (at ch 0))
     (rx : i64 (at ch 1))
  (do
    (channel-send tx 1)
    (channel-send tx 2)
    (channel-send tx 3)
    (channel-close rx)
    (channel-close tx)
    "ok"))
```

Expected: `"ok"` without memory leaks (verify with valgrind or ASAN in CI).
