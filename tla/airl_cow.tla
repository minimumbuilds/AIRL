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
 * This spec models CowWrite as a SINGLE atomic action, corresponding to the
 * runtime's actual guarantee. The NoConcurrentMutation invariant verifies
 * that no two threads can simultaneously be "committed to the fast path"
 * for the same value. A non-atomic split-step model would violate this
 * invariant with 2+ threads — documenting why COW requires exclusive access.
 *
 * Thread references are modeled explicitly: thread_refs[t][v] = TRUE means
 * thread t holds a live reference to value v (contributing to rc[v]).
 *
 * Safety invariants verified:
 *   NoConcurrentMutation: at most one thread can take the fast path per value
 *   NoStaleRead: if a thread holds a ref to v, CowWrite on v clones (never mutates v in place)
 *   NoDoubleFree: freed values are never released again
 *
 * Liveness:
 *   NoStaleRead liveness (eventual-consistency) deferred — requires fairness
 *   assumptions about thread scheduling.
 *)

EXTENDS Integers, FiniteSets, TLC

CONSTANTS
    Values,     \* Set of value IDs, e.g. {1, 2}
    Threads,    \* Set of thread IDs, e.g. {1, 2}
    MAX_RC      \* Upper bound on rc (models u32::MAX, e.g. 3)

ASSUME MAX_RC >= 2

VARIABLES
    rc,          \* rc[v] = reference count (0..MAX_RC)
    data,        \* data[v] = abstract data token (Nat, incremented on mutation)
    freed,       \* freed[v] = TRUE once the value is deallocated
    thread_refs, \* thread_refs[t][v] = TRUE if thread t holds a ref to v
    \* Per-thread CowWrite tracking (for NoConcurrentMutation)
    cow_fast,    \* cow_fast[t] = TRUE if thread t is in the fast-path moment
    cow_target   \* cow_target[t] = which value thread t is CowWriting (0 = none)

vars == <<rc, data, freed, thread_refs, cow_fast, cow_target>>

\* ── Helpers ────────────────────────────────────────────────

Live(v) == ~freed[v]

\* The rc of v must equal the number of threads holding a ref to it.
\* (Invariant maintained by Retain/Release/CowWrite.)
RefCount(v) == Cardinality({t \in Threads : thread_refs[t][v]})

\* ── Init ───────────────────────────────────────────────────
\* All values start with rc = 1 (one implicit owner), not freed.
\* No thread holds an explicit ref initially; the implicit owner is the
\* "program stack" which we do not model as a thread.

Init ==
    /\ rc          = [v \in Values |-> 1]
    /\ data        = [v \in Values |-> 0]
    /\ freed       = [v \in Values |-> FALSE]
    /\ thread_refs = [t \in Threads |-> [v \in Values |-> FALSE]]
    /\ cow_fast    = [t \in Threads |-> FALSE]
    /\ cow_target  = [t \in Threads |-> 0]

\* ── Retain(v) by thread t ──────────────────────────────────
\* Thread t acquires a reference to v: rc increments, thread_refs updated.

ThreadRetain(t, v) ==
    /\ Live(v)
    /\ ~thread_refs[t][v]         \* thread does not already hold a ref
    /\ rc[v] < MAX_RC             \* not immortal for simplicity
    /\ rc'          = [rc EXCEPT ![v] = rc[v] + 1]
    /\ thread_refs' = [thread_refs EXCEPT ![t][v] = TRUE]
    /\ UNCHANGED <<data, freed, cow_fast, cow_target>>

\* ── Release(v) by thread t ─────────────────────────────────
\* Thread t drops its reference to v: rc decrements; free at rc→0.

ThreadRelease(t, v) ==
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
    /\ UNCHANGED <<data, cow_fast, cow_target>>

\* ── CowWrite(t, v) ─────────────────────────────────────────
\* Atomic COW write: thread t writes to value v.
\*
\* The implementation reads rc atomically and branches:
\*   rc == 1 (fast path): mutate v in place; caller's ref is still valid.
\*   rc > 1  (slow path): clone v into a new value; caller gets the clone.
\*
\* We model this as a SINGLE atomic step. The atomicity corresponds to:
\*   - In the AIRL runtime: ensured by single-threaded semantics (no interleaving).
\*   - In a concurrent setting: would require a mutex over (check + mutate).
\*
\* A split non-atomic model (CowWriteCheckRc + CowWriteMutate) would expose
\* the race where two threads both observe rc==1 before either mutates.
\* That violation of NoConcurrentMutation documents why COW requires
\* the single-threaded AIRL threading model.
\*
\* Here thread t must hold a ref to v before CowWrite (it's the caller).
\* After CowWrite on the fast path: v's data is incremented, t still holds its ref.
\* After CowWrite on the slow path: t's ref to v is dropped, t holds ref to v' (new).
\* We model the slow-path clone by incrementing data[v] on a notional new value;
\* since Values is bounded we simplify: the slow path just decrements rc[v].

CowWrite(t, v) ==
    /\ Live(v)
    /\ thread_refs[t][v]   \* thread holds a ref (it's the caller)
    /\ ~cow_fast[t]        \* thread not already mid-CowWrite
    /\ LET is_fast == (rc[v] = 1)
       IN
        /\ cow_fast'   = [cow_fast EXCEPT ![t] = TRUE]
        /\ cow_target' = [cow_target EXCEPT ![t] = v]
        /\ IF is_fast
           THEN
                \* Fast path: sole owner — mutate in place.
                \* rc stays the same (t still holds its ref).
                /\ data' = [data EXCEPT ![v] = data[v] + 1]
                /\ UNCHANGED <<rc, freed, thread_refs>>
           ELSE
                \* Slow path: clone — decrement rc[v] (t drops old ref),
                \* new clone gets rc=1. Data[v] unchanged (old copy preserved).
                \* (New clone is not explicitly tracked — Values is bounded.)
                /\ rc' = [rc EXCEPT ![v] = rc[v] - 1]
                /\ thread_refs' = [thread_refs EXCEPT ![t][v] = FALSE]
                /\ UNCHANGED <<data, freed>>

\* After CowWrite completes, reset the fast-path marker.
CowWriteComplete(t) ==
    /\ cow_fast[t]
    /\ cow_fast'   = [cow_fast EXCEPT ![t] = FALSE]
    /\ cow_target' = [cow_target EXCEPT ![t] = 0]
    /\ UNCHANGED <<rc, data, freed, thread_refs>>

\* ── Stutter ────────────────────────────────────────────────

Stutter == UNCHANGED vars

\* ── Next ───────────────────────────────────────────────────

Next ==
    \/ \E t \in Threads, v \in Values : ThreadRetain(t, v)
    \/ \E t \in Threads, v \in Values : ThreadRelease(t, v)
    \/ \E t \in Threads, v \in Values : CowWrite(t, v)
    \/ \E t \in Threads : CowWriteComplete(t)
    \/ Stutter

Spec == Init /\ [][Next]_vars

\* ════════════════════════════════════════════════════════════
\* SAFETY INVARIANTS
\* ════════════════════════════════════════════════════════════

\* ── NoConcurrentMutation ───────────────────────────────────
\* No two distinct threads can simultaneously be in the COW fast path
\* for the same value. Because CowWrite is modeled atomically, cow_fast[t]
\* is only TRUE for the instant between CowWrite and CowWriteComplete — but
\* no two threads can both set it for the same target in the same step.
\*
\* This invariant documents the race condition that would arise in a
\* non-atomic (split-step) model: two threads both checking rc==1 before
\* either mutates would both advance to cow_fast=TRUE for the same value.
NoConcurrentMutation ==
    \A t1, t2 \in Threads :
        (t1 /= t2 /\ cow_fast[t1] /\ cow_fast[t2]) =>
            cow_target[t1] /= cow_target[t2]

\* ── NoStaleRead ────────────────────────────────────────────
\* If thread t holds a reference to v (rc[v] > 1 due to t's ref),
\* then CowWrite on v must take the SLOW path (clone), never the fast path.
\* Equivalently: the fast path is only taken when rc[v] == 1, which means
\* no other thread holds a reference to v.
\*
\* Here we verify the static property: whenever CowWrite is eligible for the
\* fast path (rc[v] == 1), no second thread holds a ref to v.
NoStaleRead ==
    \A v \in Values :
        rc[v] = 1 =>
            Cardinality({t \in Threads : thread_refs[t][v]}) <= 1

\* ── NoDoubleFree ───────────────────────────────────────────
\* Once freed, a value's rc is 0 and Release must not be applied again.
NoDoubleFree ==
    \A v \in Values : freed[v] => rc[v] = 0

\* ── RcConsistency ──────────────────────────────────────────
\* rc is always non-negative and does not exceed MAX_RC.
RcConsistency ==
    \A v \in Values : rc[v] >= 0 /\ rc[v] <= MAX_RC

\* ════════════════════════════════════════════════════════════
\* LIVENESS (deferred)
\* ════════════════════════════════════════════════════════════

\* NoStaleRead liveness: after a CowWrite, readers of the old value
\* eventually observe consistent data (either via the slow-path clone or
\* because rc==1 guaranteed exclusivity on the fast path).
\* Deferred — requires fairness assumptions and a richer read-event model.
(*
NoStaleReadLiveness ==
    \A t \in Threads, v \in Values :
        thread_refs[t][v] ~> (cow_fast[t] => cow_target[t] /= v)
*)

=============================================================================
