------------------------------ MODULE airl_cow --------------------------------
(*
 * TLA+ model of AIRL's COW (Copy-on-Write) fast-path optimization.
 *
 * When a map or list has rc == 1 (sole owner), AIRL mutates it in place
 * instead of cloning. This is O(1) vs O(N) for the clone path.
 *
 * Based on: crates/airl-rt/src/map.rs  airl_map_set / airl_map_remove
 *           crates/airl-rt/src/list.rs airl_list_append / airl_list_cons
 *
 * Key implementation pattern (from airl_map_set, lines 121–138):
 *   if unsafe { (*m).rc.load(Ordering::Relaxed) } == 1 {
 *       let v = unsafe { &mut *m };   // fast path: mutate in place
 *       ...
 *   } else {
 *       let v = unsafe { &*m };       // slow path: clone
 *       ...
 *   }
 *
 * The safety requirement: the rc==1 check and the mutation must be atomic
 * from the perspective of any concurrent observer. If two threads both
 * observe rc==1 before either mutates, both take the fast path and corrupt
 * the value. This is safe ONLY under AIRL's single-threaded semantics.
 *
 * This spec models CowWrite as a SINGLE atomic action — corresponding to the
 * runtime's actual guarantee. A non-atomic split-step model (CowWriteCheckRc
 * + CowWriteMutate) would violate NoConcurrentMutation with 2+ threads,
 * documenting why COW requires the AIRL single-threaded threading model or
 * explicit mutual exclusion.
 *
 * Thread references are modeled explicitly: thread_refs[t][v] = TRUE means
 * thread t holds a live reference to value v (contributing to rc[v]).
 *
 * cow_is_fast[t] and cow_orig_data[t] snapshot the path taken and the
 * original data token at the moment CowWrite fires. This allows CloneCorrectness
 * to assert that the original value is unchanged on the slow path.
 *
 * Safety invariants verified:
 *   NoConcurrentMutation: at most one thread takes the fast path per value
 *   NoDoubleFree: freed values are never released again
 *   CloneCorrectness: after a slow-path CowWrite, the original value's data
 *                     is unchanged (the clone preserves the original)
 *
 * Deferred:
 *   NoStaleRead: readers see consistent data after CowWrite — requires a
 *                read operation and fairness assumptions not yet in scope
 *)

EXTENDS Integers, FiniteSets, TLC

CONSTANTS
    Values,     \* Set of value IDs, e.g. {1, 2}
    Threads,    \* Set of thread IDs, e.g. {1, 2}
    MAX_RC      \* Upper bound on rc (models u32::MAX, e.g. 3)

ASSUME MAX_RC >= 2

VARIABLES
    rc,           \* rc[v] = reference count (0..MAX_RC)
    data,         \* data[v] = abstract data token (Nat, incremented on fast-path mutation)
    freed,        \* freed[v] = TRUE once the value is deallocated
    thread_refs,  \* thread_refs[t][v] = TRUE if thread t holds a ref to v
    cow_fast,     \* cow_fast[t] = TRUE while thread t is mid-CowWrite
    cow_target,   \* cow_target[t] = value being CowWritten (0 = none)
    cow_is_fast,  \* cow_is_fast[t] = TRUE if the CowWrite took the fast path
    cow_orig_data \* cow_orig_data[t] = data[v] snapshot at CowWrite time (for CloneCorrectness)

vars == <<rc, data, freed, thread_refs, cow_fast, cow_target, cow_is_fast, cow_orig_data>>

\* ── Helpers ────────────────────────────────────────────────

Live(v) == ~freed[v]

\* ── Init ───────────────────────────────────────────────────
\* All values start with rc = 1 (one implicit owner), not freed, data token = 0.
\* No thread holds an explicit ref initially.

Init ==
    /\ rc           = [v \in Values |-> 1]
    /\ data         = [v \in Values |-> 0]
    /\ freed        = [v \in Values |-> FALSE]
    /\ thread_refs  = [t \in Threads |-> [v \in Values |-> FALSE]]
    /\ cow_fast     = [t \in Threads |-> FALSE]
    /\ cow_target   = [t \in Threads |-> 0]
    /\ cow_is_fast  = [t \in Threads |-> FALSE]
    /\ cow_orig_data = [t \in Threads |-> 0]

\* ── Retain(v) by thread t ──────────────────────────────────
\* Thread t acquires a reference to v: rc increments, thread_refs updated.

Retain(t, v) ==
    /\ Live(v)
    /\ ~thread_refs[t][v]         \* thread does not already hold a ref
    /\ rc[v] < MAX_RC             \* not at the immortal ceiling
    /\ rc'          = [rc EXCEPT ![v] = rc[v] + 1]
    /\ thread_refs' = [thread_refs EXCEPT ![t][v] = TRUE]
    /\ UNCHANGED <<data, freed, cow_fast, cow_target, cow_is_fast, cow_orig_data>>

\* ── Release(v) by thread t ─────────────────────────────────
\* Thread t drops its reference to v: rc decrements; free at rc→0.

Release(t, v) ==
    /\ Live(v)
    /\ thread_refs[t][v]          \* thread must hold a ref
    /\ thread_refs' = [thread_refs EXCEPT ![t][v] = FALSE]
    /\ IF rc[v] = 1
       THEN \* last reference — free the value
            /\ freed' = [freed EXCEPT ![v] = TRUE]
            /\ rc'    = [rc EXCEPT ![v] = 0]
       ELSE
            /\ rc' = [rc EXCEPT ![v] = rc[v] - 1]
            /\ UNCHANGED freed
    /\ UNCHANGED <<data, cow_fast, cow_target, cow_is_fast, cow_orig_data>>

\* ── CowWrite(t, v) ─────────────────────────────────────────
\* Atomic COW write: thread t writes to value v.
\*
\* The implementation reads rc atomically and branches:
\*   rc == 1 (fast path): mutate v in place; data[v] incremented.
\*   rc > 1  (slow path): clone v; original data[v] is preserved; rc[v] decremented.
\*
\* Modeled as a SINGLE atomic step. The atomicity mirrors:
\*   - AIRL runtime: guaranteed by single-threaded semantics.
\*   - Hypothetical concurrent setting: would require a mutex.
\*
\* cow_is_fast[t] and cow_orig_data[t] are set here so CloneCorrectness can
\* assert that the original value is unchanged while t is mid slow-path.

CowWrite(t, v) ==
    /\ Live(v)
    /\ thread_refs[t][v]   \* thread holds a ref to v (it's the caller)
    /\ ~cow_fast[t]        \* thread is not already mid-CowWrite
    /\ LET is_fast == (rc[v] = 1)
       IN
        /\ cow_fast'      = [cow_fast EXCEPT ![t] = TRUE]
        /\ cow_target'    = [cow_target EXCEPT ![t] = v]
        /\ cow_is_fast'   = [cow_is_fast EXCEPT ![t] = is_fast]
        /\ cow_orig_data' = [cow_orig_data EXCEPT ![t] = data[v]]
        /\ IF is_fast
           THEN
                \* Fast path: sole owner — mutate in place.
                \* rc is unchanged (thread's ref is retained by the caller).
                /\ data' = [data EXCEPT ![v] = data[v] + 1]
                /\ UNCHANGED <<rc, freed, thread_refs>>
           ELSE
                \* Slow path: shared value — clone.
                \* Thread drops its ref to v; new clone has rc=1 (not tracked separately).
                \* data[v] is preserved (old value unchanged — CloneCorrectness verifies this).
                /\ rc' = [rc EXCEPT ![v] = rc[v] - 1]
                /\ thread_refs' = [thread_refs EXCEPT ![t][v] = FALSE]
                /\ UNCHANGED <<data, freed>>

\* After CowWrite completes, reset the per-thread tracking variables.
CowWriteComplete(t) ==
    /\ cow_fast[t]
    /\ cow_fast'      = [cow_fast EXCEPT ![t] = FALSE]
    /\ cow_target'    = [cow_target EXCEPT ![t] = 0]
    /\ cow_is_fast'   = [cow_is_fast EXCEPT ![t] = FALSE]
    /\ cow_orig_data' = [cow_orig_data EXCEPT ![t] = 0]
    /\ UNCHANGED <<rc, data, freed, thread_refs>>

\* ── Stutter ────────────────────────────────────────────────

Stutter == UNCHANGED vars

\* ── Next ───────────────────────────────────────────────────

Next ==
    \/ \E t \in Threads, v \in Values : Retain(t, v)
    \/ \E t \in Threads, v \in Values : Release(t, v)
    \/ \E t \in Threads, v \in Values : CowWrite(t, v)
    \/ \E t \in Threads : CowWriteComplete(t)
    \/ Stutter

Spec == Init /\ [][Next]_vars

\* ════════════════════════════════════════════════════════════
\* SAFETY INVARIANTS
\* ════════════════════════════════════════════════════════════

\* ── NoConcurrentMutation ───────────────────────────────────
\* No two distinct threads can simultaneously be in the COW fast path
\* for the same value. Because CowWrite is modeled as a single atomic step,
\* cow_fast[t] is TRUE only between CowWrite and CowWriteComplete. Two threads
\* cannot both set cow_fast for the same target in the same atomic step.
\*
\* This invariant documents the race that would arise in a non-atomic model:
\* both threads observe rc==1 before either mutates → both take fast path.
NoConcurrentMutation ==
    \A t1, t2 \in Threads :
        (t1 /= t2 /\ cow_fast[t1] /\ cow_fast[t2] /\ cow_is_fast[t1] /\ cow_is_fast[t2]) =>
            cow_target[t1] /= cow_target[t2]

\* ── NoDoubleFree ───────────────────────────────────────────
\* Once freed, a value's rc is 0. Release must not be applied again.
\* (Same invariant as in airl_memory.tla.)
NoDoubleFree ==
    \A v \in Values : freed[v] => rc[v] = 0

\* ── CloneCorrectness ───────────────────────────────────────
\* After a slow-path CowWrite, the original value's data is unchanged.
\* While thread t is mid slow-path CowWrite on value v (cow_fast[t] = TRUE,
\* cow_is_fast[t] = FALSE), data[v] must equal the snapshot taken at write time.
\*
\* This verifies that the implementation's slow path uses `UNCHANGED data` —
\* only the fast path (exclusive owner) may mutate the data token.
CloneCorrectness ==
    \A t \in Threads :
        (cow_fast[t] /\ ~cow_is_fast[t]) =>
            data[cow_target[t]] = cow_orig_data[t]

\* ── RcConsistency ──────────────────────────────────────────
\* rc is always non-negative and does not exceed MAX_RC.
RcConsistency ==
    \A v \in Values : rc[v] >= 0 /\ rc[v] <= MAX_RC

\* ════════════════════════════════════════════════════════════
\* DEFERRED (not checked by TLC in MC_cow.cfg)
\* ════════════════════════════════════════════════════════════

\* ── NoStaleRead ────────────────────────────────────────────
\* After a CowWrite completes, any thread holding a reference to the original
\* value should see consistent (non-mutated) data. On the fast path this is
\* guaranteed by exclusivity (rc==1). On the slow path the original is cloned
\* and left unchanged, so existing readers see the pre-write data.
\*
\* Deferred: verifying this property requires modeling explicit Read operations
\* (not yet in scope) and fairness assumptions about thread scheduling.
(*
NoStaleRead ==
    \A t \in Threads, v \in Values :
        thread_refs[t][v] ~> (cow_fast[t] => cow_target[t] /= v)
*)

=============================================================================
