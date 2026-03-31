# TLA+ Formal Verification Report

**Date:** 2026-03-31
**Scope:** AIRLOS microkernel, AIRL runtime, Airline async framework

## Overview

TLA+ model checking was applied to three concurrent subsystems across the AIRL ecosystem. The TLC model checker exhaustively explored all reachable states to verify safety properties and find bugs.

**Total: 5 real bugs found, 2 fixed, 3 specs written for pending fixes.**

## Results Summary

| Repo | Spec | States Checked | Bugs Found | Status |
|------|------|---------------|------------|--------|
| AIRLOS | `ipc_rendezvous.tla` | 551 | 2 | Fixed in kernel |
| AIRLOS | `ipc_rendezvous_fixed.tla` | 5,019,869 | 0 | Fixes verified |
| AIRLOS | `ipc_async.tla` | 15M+ | 1 (by design) | Documented |
| AIRL | `airl_channels.tla` | 13.5M+ | 3 | Specs written |
| Airline | `airline_reactors.tla` | 15M+ | 0 | Design is sound |

---

## AIRLOS Microkernel IPC

### Spec: `tla/ipc_rendezvous.tla`

Models synchronous IPC: `ipc_send`, `ipc_recv`, `sys_sendrecv` with 3 tasks.

#### Bug 1: Stuck on Dead Target (HIGH)

**Trace:**
```
Step 1: All 3 tasks READY
Step 2: Task 2 sends to Task 1 → Task 1 not receiving → Task 2 BLOCKS
Step 3: Task 1 DIES
Result: Task 2 is SEND_BLOCKED forever, waiting for a dead task
```

**Root cause:** `ipc.c` has no cleanup when a task dies. Tasks blocked on IPC with the dying task are never woken.

**Fix applied:** `ipc_wake_blocked_on()` in `ipc.c` — scans task list on task death, wakes blocked tasks with `E_NOTASK`. Called from both `sys_exit` (syscall.c) and `task_exit` (task.c).

#### Bug 2: Send Cycle Deadlock (CRITICAL)

**Trace:**
```
Step 1: All tasks READY
Step 2: Task 2 sends to Task 1 → blocks
Step 3: Task 3 sends to Task 2 → blocks
Step 4: Task 1 sends to Task 3 → blocks
Result: All 3 tasks SEND_BLOCKED in cycle 1→3→2→1. Permanent deadlock.
```

**Root cause:** `ipc_send` only rejects self-deadlock (`dest == self`), not transitive cycles.

**Fix applied:** `ipc_would_deadlock()` in `ipc.c` — walks the send-blocked chain before blocking. Returns `E_DEADLOCK` if a cycle would form. New error code `E_DEADLOCK` (-35) in `ipc_types.h`.

#### Verification of Fixes

`ipc_rendezvous_fixed.tla` models both fixes. TLC checked 5,019,869 distinct states in 1m 51s. All invariants pass:
- `NoStuckOnDead` — no task blocked on a dead target
- `NoCycleDeadlock` — no 3-task send cycle
- `MessageIntegrity` — every delivered message has valid sender and sequence number

### Spec: `tla/ipc_async.tla`

Models asynchronous IPC (single-slot buffer) and notification bits.

#### Finding: Message Loss by Design (MEDIUM)

**Trace:**
```
Step 1: Task 2 sends async to Task 1 → slot filled
Step 2: Task 3 sends async to Task 1 → slot FULL → E_AGAIN
Result: Task 3's message is silently lost
```

This is the expected behavior of the single-slot design. The kernel returns `E_AGAIN` and callers must retry. Safety invariants (slot consistency, message integrity, notification bit validity) all pass across 15M+ states.

**Files:**
- `AIRLOS/tla/ipc_rendezvous.tla` — buggy model
- `AIRLOS/tla/ipc_rendezvous_fixed.tla` — fixed model
- `AIRLOS/tla/ipc_async.tla` — async + notifications model
- `AIRLOS/tla/MC_rendezvous.cfg` — TLC config (buggy)
- `AIRLOS/tla/MC_rendezvous_fixed.cfg` — TLC config (fixed)
- `AIRLOS/tla/MC_async.cfg` — TLC config (async)
- `AIRLOS/src/kernel/ipc.c` — kernel fixes (cycle detection + wake-on-death)
- `AIRLOS/src/include/ipc_types.h` — E_DEADLOCK error code

---

## AIRL Runtime Channels

### Spec: `tla/airl_channels.tla`

Models AIRL's thread + channel primitives: `channel-new`, `channel-send`, `channel-recv`, `channel-drain`, `channel-close`.

#### Bug 1: Concurrent Recv Race (HIGH)

**Trace:**
```
Step 1: Thread 1 calls channel-recv → queue empty → blocks
        (rx REMOVED from HashMap during recv)
Step 2: Thread 2 calls channel-recv on SAME channel →
        rx not in HashMap → "invalid receiver handle"
```

**Root cause:** `crates/airl-rt/src/thread.rs` lines 180-188 use a `remove()` → `recv()` → `insert()` pattern. The receiver is absent from the global map during the entire blocking `recv()`. Same bug exists in `channel-recv-timeout` and `channel-drain`.

**Fix spec:** `docs/superpowers/specs/2026-03-31-channel-recv-race-fix.md`
Replace `HashMap<i64, Receiver>` with `HashMap<i64, Arc<Mutex<Receiver>>>`.

#### Bug 2: Misleading Send-After-Close Error (LOW)

**Trace:**
```
Step 1: Thread 1 closes sender handle → removed from HashMap
Step 2: Thread 1 sends on same channel → "invalid sender handle 5"
```

**Root cause:** `channel-close` removes the handle from the map. Subsequent sends get the same error as a typo'd handle number — "invalid sender handle" instead of "channel closed".

**Fix spec:** `docs/superpowers/specs/2026-03-31-channel-close-error-clarity.md`
Track closed handles in a separate `HashSet`. Check it before the active map.

#### Bug 3: Silent Message Loss on Receiver Close (MEDIUM)

**Trace:**
```
Step 1: Thread 1 sends message → queued in channel buffer
Step 2: Thread 1 closes receiver → Receiver dropped, buffer dropped
Result: Message silently lost. Memory leaked (RtValue refcount never decremented).
```

**Root cause:** `channel-close` drops the `Receiver`, which drops all buffered `SendPtr` wrappers without calling `airl_value_release` on the contained `RtValue` pointers.

**Fix spec:** `docs/superpowers/specs/2026-03-31-channel-recv-close-message-loss.md`
Drain and release all buffered messages before dropping the receiver.

#### Safety Invariants Verified (13.5M+ states)

- `MessageIntegrity` — every delivered message has valid sender + sequence number
- `NoDoubleDelivery` — no message delivered to two different threads
- `FIFOPerSender` — messages from the same sender arrive in send order

**Files:**
- `AIRL/tla/airl_channels.tla` — channel model
- `AIRL/tla/MC_channels.cfg` — TLC config
- `AIRL/docs/superpowers/specs/2026-03-31-channel-recv-race-fix.md`
- `AIRL/docs/superpowers/specs/2026-03-31-channel-close-error-clarity.md`
- `AIRL/docs/superpowers/specs/2026-03-31-channel-recv-close-message-loss.md`

---

## Airline Async Framework

### Spec: `tla/airline_reactors.tla`

Models per-core reactors, cross-core task submission, work stealing, and shutdown.

#### Result: No Bugs Found

TLC explored 15M+ states with 0 invariant violations:

- `NoDoubleExecution` — stolen tasks never run on both the original and stealing core
- `TaskIntegrity` — every completed task has a valid result
- `NoTaskLost` — no tasks disappear from the system
- `FutureIntegrity` — every resolved future corresponds to a completed task
- `MailboxIntegrity` — all cross-core messages have valid format

**Why it's clean:** Airline's reactors are single-threaded event loops. Each reactor executes drain → run → steal sequentially. There is no concurrent access to a reactor's task queue — work stealing happens via message passing (channel-send to the target's mailbox), so the stealing core never directly modifies the victim's queue. This is the same safe pattern as AIRLOS's rendezvous IPC.

**Files:**
- `airline/tla/airline_reactors.tla` — reactor model
- `airline/tla/MC_reactors.cfg` — TLC config

---

## Running the Specs

### Prerequisites

```bash
# Java 17+ required
java -version

# Download TLA+ tools (one-time, ~4MB)
wget https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar
```

### Commands

```bash
# AIRLOS — verify IPC fixes
cd AIRLOS/tla
java -XX:+UseParallelGC -jar tla2tools.jar -config MC_rendezvous_fixed.cfg ipc_rendezvous_fixed.tla

# AIRL — check channel model
cd AIRL/tla
java -XX:+UseParallelGC -jar tla2tools.jar -config MC_channels.cfg airl_channels.tla

# Airline — verify reactor model
cd airline/tla
java -XX:+UseParallelGC -jar tla2tools.jar -config MC_reactors.cfg airline_reactors.tla
```

### When to Re-run

| Change | Re-run |
|--------|--------|
| Modified `AIRLOS/src/kernel/ipc.c` or `notify.c` | AIRLOS IPC specs |
| Modified `AIRL/crates/airl-rt/src/thread.rs` | `airl_channels.tla` |
| Modified `airline/reactor.airl` or `steal.airl` | `airline_reactors.tla` |
| Added new blocking/concurrent primitive | Write new spec or extend existing |
| No concurrent code changed | Don't re-run |

---

## Pending Work

| Item | Priority | Effort |
|------|----------|--------|
| Fix AIRL channel recv race (Bug 1) | High | ~30 lines in thread.rs |
| Fix AIRL channel close message loss (Bug 3) | Medium | ~15 lines in thread.rs |
| Fix AIRL channel close error clarity (Bug 2) | Low | ~10 lines in thread.rs |
| Spec 3: AIRLOS service lifecycle model | Future | New TLA+ spec |
| AIRL_castle Kafka consumer group model | Future | New TLA+ spec |
