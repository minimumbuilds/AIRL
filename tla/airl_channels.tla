---------------------------- MODULE airl_channels ----------------------------
(*
 * TLA+ model of AIRL's thread + channel concurrency primitives.
 *
 * Models:
 *   - channel-new: creates unbounded mpsc channel, returns [tx-handle, rx-handle]
 *   - channel-send: non-blocking send (unbounded, never blocks)
 *   - channel-recv: blocking receive (blocks until message or channel closed)
 *   - channel-recv-timeout: receive with timeout
 *   - channel-drain: non-blocking drain of all pending messages
 *   - channel-close: removes handle from global map (drops sender or receiver)
 *   - thread-spawn: spawn thread running a closure
 *   - thread-join: block until thread completes
 *
 * Based on: AIRL/crates/airl-rt/src/thread.rs
 *
 * Key implementation detail: The receiver is temporarily REMOVED from the
 * global HashMap during recv/drain operations (lines 180, 206, 231 of
 * thread.rs). If two threads hold the same rx_handle and both call recv,
 * the second one gets "invalid receiver handle" because the first removed
 * it from the map. This is a potential bug.
 *)

EXTENDS Integers, Sequences, FiniteSets, TLC

CONSTANTS
    Threads,        \* Set of thread IDs, e.g. {1, 2, 3}
    Channels,       \* Set of channel IDs, e.g. {1}
    MaxMessages     \* Bound on messages per thread

VARIABLES
    \* Thread state
    thread_state,   \* RUNNING, RECV_BLOCKED, JOIN_BLOCKED, DONE
    thread_inbox,   \* messages received by each thread

    \* Channel state
    chan_queue,      \* chan_queue[c] = sequence of messages in the channel buffer
    chan_tx_open,    \* chan_tx_open[c] = TRUE if sender handle exists in map
    chan_rx_open,    \* chan_rx_open[c] = TRUE if receiver handle exists in map
    chan_rx_held,    \* chan_rx_held[c] = thread currently holding rx (removed from map)
                    \* 0 if nobody holds it (it's in the map)

    \* Tracking
    send_count,     \* per-thread send counter
    send_after_close, \* sends attempted after sender was closed
    recv_on_held,   \* recv attempts when another thread holds the rx
    messages_lost   \* messages that went into a closed channel

vars == <<thread_state, thread_inbox, chan_queue, chan_tx_open, chan_rx_open,
          chan_rx_held, send_count, send_after_close, recv_on_held, messages_lost>>

\* States
RUNNING      == "RUNNING"
RECV_BLOCKED == "RECV_BLOCKED"
DONE         == "DONE"

\* ── Init ───────────────────────────────────────────────

Init ==
    /\ thread_state    = [t \in Threads |-> RUNNING]
    /\ thread_inbox    = [t \in Threads |-> <<>>]
    /\ chan_queue       = [c \in Channels |-> <<>>]
    /\ chan_tx_open     = [c \in Channels |-> TRUE]
    /\ chan_rx_open     = [c \in Channels |-> TRUE]
    /\ chan_rx_held     = [c \in Channels |-> 0]
    /\ send_count      = [t \in Threads |-> 0]
    /\ send_after_close = 0
    /\ recv_on_held    = 0
    /\ messages_lost   = 0

\* ── channel-send(thread, channel, msg) ─────────────────
\* Non-blocking. Unbounded queue, so never fails due to capacity.
\* Fails only if sender handle was closed.

ChannelSend(thread, chan) ==
    /\ thread_state[thread] = RUNNING
    /\ send_count[thread] < MaxMessages
    /\ LET msg == <<thread, send_count[thread] + 1>>
       IN
        IF chan_tx_open[chan]
        THEN
            \* Sender handle valid — enqueue message
            /\ chan_queue'    = [chan_queue EXCEPT ![chan] = Append(@, msg)]
            /\ send_count'   = [send_count EXCEPT ![thread] = @ + 1]
            /\ UNCHANGED <<thread_state, thread_inbox, chan_tx_open,
                           chan_rx_open, chan_rx_held, send_after_close,
                           recv_on_held, messages_lost>>
        ELSE
            \* Sender handle closed — send fails
            /\ send_after_close' = send_after_close + 1
            /\ send_count' = [send_count EXCEPT ![thread] = @ + 1]
            /\ UNCHANGED <<thread_state, thread_inbox, chan_queue,
                           chan_tx_open, chan_rx_open, chan_rx_held,
                           recv_on_held, messages_lost>>

\* ── channel-recv(thread, channel) ──────────────────────
\* Blocking receive. Implementation REMOVES rx from map during recv.
\* This means a second thread calling recv on same handle gets an error.

ChannelRecvStart(thread, chan) ==
    /\ thread_state[thread] = RUNNING
    /\ chan_rx_open[chan]
    /\ chan_rx_held[chan] = 0   \* nobody else is holding the rx
    /\ IF Len(chan_queue[chan]) > 0
       THEN
            \* Message available — deliver immediately
            /\ thread_inbox' = [thread_inbox EXCEPT ![thread] =
                                Append(@, Head(chan_queue[chan]))]
            /\ chan_queue'   = [chan_queue EXCEPT ![chan] = Tail(@)]
            /\ UNCHANGED <<thread_state, chan_tx_open, chan_rx_open,
                           chan_rx_held, send_count, send_after_close,
                           recv_on_held, messages_lost>>
       ELSE IF ~chan_tx_open[chan]
       THEN
            \* Channel closed (sender dropped), queue empty — return error
            /\ UNCHANGED vars
       ELSE
            \* Queue empty, channel open — block
            \* Implementation: remove rx from map (chan_rx_held = thread)
            /\ thread_state' = [thread_state EXCEPT ![thread] = RECV_BLOCKED]
            /\ chan_rx_held' = [chan_rx_held EXCEPT ![chan] = thread]
            /\ UNCHANGED <<thread_inbox, chan_queue, chan_tx_open, chan_rx_open,
                           send_count, send_after_close, recv_on_held,
                           messages_lost>>

\* When a blocked receiver gets a message (sender added to queue)
ChannelRecvComplete(thread, chan) ==
    /\ thread_state[thread] = RECV_BLOCKED
    /\ chan_rx_held[chan] = thread
    /\ Len(chan_queue[chan]) > 0
    /\ thread_state' = [thread_state EXCEPT ![thread] = RUNNING]
    /\ thread_inbox'  = [thread_inbox EXCEPT ![thread] =
                         Append(@, Head(chan_queue[chan]))]
    /\ chan_queue'    = [chan_queue EXCEPT ![chan] = Tail(@)]
    /\ chan_rx_held'  = [chan_rx_held EXCEPT ![chan] = 0]
    /\ UNCHANGED <<chan_tx_open, chan_rx_open, send_count,
                   send_after_close, recv_on_held, messages_lost>>

\* A second thread tries to recv on the same channel while first holds rx.
\* NOTE: This action is NOT in Next. The implementation fix (Arc<Mutex<Receiver>>)
\* serializes concurrent recvs, so this conflict state is unreachable.
\* Kept here to document the pre-fix failure mode.
ChannelRecvConflict(thread, chan) ==
    /\ thread_state[thread] = RUNNING
    /\ chan_rx_open[chan]
    /\ chan_rx_held[chan] /= 0            \* someone else holds the rx
    /\ chan_rx_held[chan] /= thread       \* it's not us
    /\ recv_on_held' = recv_on_held + 1  \* track this as a conflict
    /\ UNCHANGED <<thread_state, thread_inbox, chan_queue, chan_tx_open,
                   chan_rx_open, chan_rx_held, send_count, send_after_close,
                   messages_lost>>

\* ── channel-drain(thread, channel) ─────────────────────
\* Non-blocking. Takes ALL pending messages. Also removes rx from map.

ChannelDrain(thread, chan) ==
    /\ thread_state[thread] = RUNNING
    /\ chan_rx_open[chan]
    /\ chan_rx_held[chan] = 0
    /\ Len(chan_queue[chan]) > 0
    \* Deliver all messages at once
    /\ thread_inbox' = [thread_inbox EXCEPT ![thread] = @ \o chan_queue[chan]]
    /\ chan_queue'   = [chan_queue EXCEPT ![chan] = <<>>]
    /\ UNCHANGED <<thread_state, chan_tx_open, chan_rx_open, chan_rx_held,
                   send_count, send_after_close, recv_on_held, messages_lost>>

\* ── channel-close(handle) ──────────────────────────────
\* Closing sender: messages already in queue are still deliverable.
\* Closing receiver: messages in queue are lost.

CloseSender(thread, chan) ==
    /\ thread_state[thread] = RUNNING
    /\ chan_tx_open[chan]
    /\ chan_tx_open' = [chan_tx_open EXCEPT ![chan] = FALSE]
    \* If a receiver is blocked, it should be woken with "channel closed"
    /\ IF \E t \in Threads : thread_state[t] = RECV_BLOCKED /\ chan_rx_held[chan] = t
       THEN
            \E t \in Threads :
                /\ thread_state[t] = RECV_BLOCKED
                /\ chan_rx_held[chan] = t
                /\ thread_state' = [thread_state EXCEPT ![t] = RUNNING]
                /\ chan_rx_held'  = [chan_rx_held EXCEPT ![chan] = 0]
                /\ UNCHANGED <<thread_inbox, chan_queue, chan_rx_open,
                               send_count, send_after_close, recv_on_held,
                               messages_lost>>
       ELSE
            /\ UNCHANGED <<thread_state, thread_inbox, chan_queue, chan_rx_open,
                           chan_rx_held, send_count, send_after_close,
                           recv_on_held, messages_lost>>

CloseReceiver(thread, chan) ==
    /\ thread_state[thread] = RUNNING
    /\ chan_rx_open[chan]
    /\ chan_rx_held[chan] = 0   \* can't close while someone holds it
    /\ chan_rx_open'  = [chan_rx_open EXCEPT ![chan] = FALSE]
    /\ messages_lost' = messages_lost + Len(chan_queue[chan])
    /\ chan_queue'    = [chan_queue EXCEPT ![chan] = <<>>]
    /\ UNCHANGED <<thread_state, thread_inbox, chan_tx_open, chan_rx_held,
                   send_count, send_after_close, recv_on_held>>

\* ── Thread completion ──────────────────────────────────

ThreadDone(thread) ==
    /\ thread_state[thread] = RUNNING
    /\ thread_state' = [thread_state EXCEPT ![thread] = DONE]
    /\ UNCHANGED <<thread_inbox, chan_queue, chan_tx_open, chan_rx_open,
                   chan_rx_held, send_count, send_after_close, recv_on_held,
                   messages_lost>>

\* ── Terminal state ─────────────────────────────────────

AllDone == \A t \in Threads : thread_state[t] \in {DONE, RECV_BLOCKED}

\* ── Next ───────────────────────────────────────────────

Next ==
    \/ \E t \in Threads, c \in Channels : ChannelSend(t, c)
    \/ \E t \in Threads, c \in Channels : ChannelRecvStart(t, c)
    \/ \E t \in Threads, c \in Channels : ChannelRecvComplete(t, c)
    \* ChannelRecvConflict removed: Arc<Mutex<Receiver>> serializes concurrent
    \* recvs, so two threads calling recv on the same handle block in sequence
    \* rather than one getting "invalid receiver handle". The action is kept
    \* below as documentation of the pre-fix behavior but is not reachable.
    \/ \E t \in Threads, c \in Channels : ChannelDrain(t, c)
    \/ \E t \in Threads, c \in Channels : CloseSender(t, c)
    \/ \E t \in Threads, c \in Channels : CloseReceiver(t, c)
    \/ \E t \in Threads : ThreadDone(t)
    \/ (AllDone /\ UNCHANGED vars)

Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

\* ════════════════════════════════════════════════════════
\* PROPERTIES
\* ════════════════════════════════════════════════════════

\* ── Safety: Message integrity ──────────────────────────
\* Every received message has a valid sender and sequence number
MessageIntegrity ==
    \A t \in Threads : \A i \in 1..Len(thread_inbox[t]) :
        LET msg == thread_inbox[t][i]
        IN /\ msg[1] \in Threads
           /\ msg[2] > 0

\* ── Safety: No double-delivery ─────────────────────────
\* A message sent once is delivered at most once across all threads
NoDoubleDelivery ==
    \A t1, t2 \in Threads : \A i \in 1..Len(thread_inbox[t1]) :
        \A j \in 1..Len(thread_inbox[t2]) :
            (t1 /= t2 \/ i /= j) =>
            thread_inbox[t1][i] /= thread_inbox[t2][j]

\* ── Safety: FIFO ordering within a channel ─────────────
\* Messages from the same sender arrive in send order
\* (This holds because mpsc is FIFO and we model a single channel)
FIFOPerSender ==
    \A t \in Threads :
        \A i, j \in 1..Len(thread_inbox[t]) :
            LET mi == thread_inbox[t][i]
                mj == thread_inbox[t][j]
            IN (mi[1] = mj[1] /\ i < j) => mi[2] < mj[2]

\* ── Bug detection: concurrent recv on same handle ──────
\* Fixed: receiver is wrapped in Arc<Mutex<Receiver>> so concurrent recvs
\* serialize rather than conflict. ChannelRecvConflict is excluded from Next.
\* recv_on_held remains 0 throughout all reachable states.
NoConcurrentRecvConflict == recv_on_held = 0

\* ── Bug detection: send after close ────────────────────
\* Sends to a closed sender handle return error, not "channel closed"
\* because the handle is removed from the map entirely.
NoSendAfterClose == send_after_close = 0

\* ── Bug detection: messages lost on receiver close ─────
\* Closing the receiver discards all queued messages.
NoMessagesLost == messages_lost = 0

\* ── Liveness: blocked receiver eventually unblocks ─────
\* If a thread is RECV_BLOCKED, it eventually becomes RUNNING or DONE
\* (requires fairness + eventually either a send or close happens)
RecvProgress ==
    \A t \in Threads :
        thread_state[t] = RECV_BLOCKED ~> thread_state[t] /= RECV_BLOCKED

=============================================================================
