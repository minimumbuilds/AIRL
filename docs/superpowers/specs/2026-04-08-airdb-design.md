# AirDB — PostgreSQL Client SDK for AIRL

**Date:** 2026-04-08
**Status:** Proposed
**Scope:** New project `repos/AirDB` + minor AIRL runtime additions

---

## Context

The AIRL ecosystem has no persistence layer. AirGate can serve web requests, AIRL_castle can publish Kafka events, AIReqL can call HTTP APIs — but there is no way to read or write to a database. Every stateful web application is blocked.

AirDB fills this gap. It implements the PostgreSQL wire protocol v3 from scratch over raw TCP, following the same layered architecture that AIRL_castle proved for Kafka. No libpq dependency, no FFI — pure AIRL compiled to a native binary via g3.

PostgreSQL is the target because:
- Wire protocol v3 is stable (unchanged since 2003), well-documented, and implementable in pure AIRL
- It is the dominant choice for AIRL-era web development
- AirGate + AirDB covers the complete backend stack
- The protocol is structurally similar to Kafka: length-prefixed binary frames over TCP

---

## Design Principles

1. **Pure AIRL, raw TCP.** No libpq, no C bindings for the protocol layer. TCP is already available via airline builtins. This mirrors AIRL_castle's approach to Kafka.
2. **Layered architecture.** Wire codec → protocol state machine → connection → query API → pool. Each layer testable in isolation.
3. **Result-typed, no exceptions.** Every fallible operation returns `(Ok value)` or `(Err reason)`. Callers are forced to handle errors.
4. **Immutable rows.** Query results are lists of Maps: `[{"id" 1 "name" "Alice"} ...]`. No cursors, no mutable result sets.
5. **Prepared statements are first-class.** The extended query protocol (Parse/Bind/Execute) is the primary path. Simple query is available but secondary.
6. **Async-ready from the start.** Phase 1 uses synchronous blocking TCP. Phase 3 wires into airline's reactor for async execution. The connection type is designed so this transition does not change the caller's API.

---

## Architecture

Following AIRL_castle's pattern:

```
┌───────────────────────────────────────────────────────────┐
│ Layer 4: Public API (airdb.airl)                          │
│   airdb-connect, airdb-query, airdb-exec, airdb-prepare   │
│   airdb-begin, airdb-commit, airdb-rollback               │
└───────────────────────────────────────────────────────────┘
                           ↓ uses
┌───────────────────────────────────────────────────────────┐
│ Layer 3: Connection (airdb-conn.airl)                     │
│   Connection state, send/recv dispatch, auth handshake    │
└───────────────────────────────────────────────────────────┘
                           ↓ uses
┌───────────────────────────────────────────────────────────┐
│ Layer 2: Protocol (airdb-protocol.airl)                   │
│   Message framing (frontend + backend), state machine     │
│   Simple query, extended query (Parse/Bind/Execute/Sync)  │
└───────────────────────────────────────────────────────────┘
                           ↓ uses
┌───────────────────────────────────────────────────────────┐
│ Layer 1: Wire Codec (airdb-wire.airl)                     │
│   Encode/decode all PG message types                      │
│   Int16/Int32 big-endian, C-string, byte arrays           │
└───────────────────────────────────────────────────────────┘
                           ↓ uses
┌───────────────────────────────────────────────────────────┐
│ Layer 0: Transport (TCP via airline builtins)             │
│   tcp-connect, tcp-send, tcp-recv, tcp-close              │
└───────────────────────────────────────────────────────────┘
```

Optional layers (Phase 3+):

```
┌───────────────────────────────────────────────────────────┐
│ Connection Pool (airdb-pool.airl)                         │
│   Min/max size, idle timeout, acquire/release             │
└───────────────────────────────────────────────────────────┘

┌───────────────────────────────────────────────────────────┐
│ Query Builder (airdb-query-builder.airl)                  │
│   Composable SELECT / INSERT / UPDATE / DELETE builders   │
└───────────────────────────────────────────────────────────┘
```

---

## PostgreSQL Wire Protocol v3 — Message Reference

### Startup Sequence

```
Client → Server: StartupMessage (version=196608, user=..., database=..., ...)
Server → Client: AuthenticationRequest (MD5 / SCRAM-SHA-256 / OK)
Client → Server: AuthenticationResponse (hashed credentials)
Server → Client: ParameterStatus (server_version, client_encoding, ...)
Server → Client: BackendKeyData (pid, secret_key)
Server → Client: ReadyForQuery (status='I')
```

### Simple Query Protocol

```
Client → Server: Query("SELECT * FROM users WHERE id = 1")
Server → Client: RowDescription ([{name:"id", type:23} {name:"name", type:25}])
Server → Client: DataRow (["1", "Alice"])
Server → Client: DataRow (["2", "Bob"])
Server → Client: CommandComplete ("SELECT 2")
Server → Client: ReadyForQuery ('I')
```

### Extended Query Protocol (Prepared Statements)

```
Client → Server: Parse(name="get_user", query="SELECT * FROM users WHERE id = $1")
Client → Server: Describe(name="get_user")
Client → Server: Sync
Server → Client: ParseComplete
Server → Client: ParameterDescription ([23])  -- INT4
Server → Client: RowDescription ([...])
Server → Client: ReadyForQuery

Client → Server: Bind(portal="", stmt="get_user", params=["42"])
Client → Server: Execute(portal="", max_rows=0)
Client → Server: Sync
Server → Client: BindComplete
Server → Client: DataRow (["42", "Carol"])
Server → Client: CommandComplete ("SELECT 1")
Server → Client: ReadyForQuery
```

### Message Types Implemented

**Frontend (client → server):**

| Type | Byte | Description |
|------|------|-------------|
| StartupMessage | — | No type byte; length-prefixed with protocol version |
| PasswordMessage | `p` | MD5 hash or SCRAM response |
| Query | `Q` | Simple query |
| Parse | `P` | Prepare a named statement |
| Bind | `B` | Bind parameters to a portal |
| Describe | `D` | Describe a statement or portal |
| Execute | `E` | Execute a portal |
| Sync | `S` | Sync (flush pipeline) |
| Close | `C` | Close a statement or portal |
| Terminate | `X` | Graceful disconnect |

**Backend (server → client):**

| Type | Byte | Description |
|------|------|-------------|
| AuthenticationOk | `R` | Auth succeeded (int32=0) |
| AuthenticationMD5Password | `R` | MD5 challenge (salt 4 bytes) |
| AuthenticationSASL | `R` | SCRAM-SHA-256 challenge |
| BackendKeyData | `K` | PID + secret key |
| ParameterStatus | `S` | Server parameter setting |
| ReadyForQuery | `Z` | Idle / in-transaction / error |
| RowDescription | `T` | Column names and types |
| DataRow | `D` | One row of data |
| CommandComplete | `C` | Statement done (tag: SELECT N, INSERT N, etc.) |
| ErrorResponse | `E` | Error fields (severity, code, message, detail, hint) |
| NoticeResponse | `N` | Non-fatal notice |
| ParseComplete | `1` | Parse succeeded |
| BindComplete | `2` | Bind succeeded |
| CloseComplete | `3` | Close succeeded |
| ParameterDescription | `t` | Parameter type OIDs for a prepared statement |
| NoData | `n` | No RowDescription follows |
| EmptyQueryResponse | `I` | Empty query string |

---

## Layer 1: Wire Codec (airdb-wire.airl)

Encode and decode individual message frames. Pure functions — no IO.

### Encoding

```airl
;; Read a big-endian int32 from a byte offset in a Bytes value
(defn wire-read-int32
  :sig [(buf : Bytes) (offset : Int) -> Int] ...)

;; Read a big-endian int16
(defn wire-read-int16
  :sig [(buf : Bytes) (offset : Int) -> Int] ...)

;; Read a null-terminated C string starting at offset
;; Returns {"value" str "offset" new-offset}
(defn wire-read-cstring
  :sig [(buf : Bytes) (offset : Int) -> Map] ...)

;; Write an int32 big-endian into a Bytes builder
(defn wire-write-int32
  :sig [(val : Int) -> Bytes] ...)

;; Write an int16 big-endian
(defn wire-write-int16
  :sig [(val : Int) -> Bytes] ...)

;; Write a null-terminated C string
(defn wire-write-cstring
  :sig [(s : String) -> Bytes] ...)

;; Build a complete backend message frame from type byte + body
;; Format: [type:1][length:4][body]  (length includes itself)
(defn wire-frame
  :sig [(msg-type : String) (body : Bytes) -> Bytes] ...)

;; Parse the type byte + length of the next message in a buffer
;; Returns {"type" "D" "length" 42 "offset" 5} or (Err "incomplete")
(defn wire-peek-message
  :sig [(buf : Bytes) (offset : Int) -> _] ...)
```

---

## Layer 2: Protocol (airdb-protocol.airl)

Encode frontend messages and decode backend messages into AIRL Maps.

### Frontend Message Encoders

```airl
(defn proto-startup-message
  :sig [(user : String) (database : String) (params : Map) -> Bytes] ...)

(defn proto-query
  :sig [(sql : String) -> Bytes] ...)

(defn proto-parse
  :sig [(name : String) (query : String) (param-types : List) -> Bytes] ...)

(defn proto-bind
  :sig [(portal : String) (stmt : String) (params : List) -> Bytes] ...)

(defn proto-describe
  :sig [(kind : String) (name : String) -> Bytes]
  ;; kind: "S" for statement, "P" for portal
  ...)

(defn proto-execute
  :sig [(portal : String) (max-rows : Int) -> Bytes] ...)

(defn proto-sync   :sig [() -> Bytes] ...)
(defn proto-flush  :sig [() -> Bytes] ...)
(defn proto-terminate :sig [() -> Bytes] ...)

(defn proto-password-message
  :sig [(password : String) -> Bytes] ...)
```

### Backend Message Decoders

Decode a raw frame (bytes at offset) into a typed Map:

```airl
(defn proto-decode-message
  :sig [(buf : Bytes) (offset : Int) -> _]
  ;; Returns (Ok {"type" "DataRow" "columns" ["1" "Alice"] ...})
  ;;      or (Err "unknown message type X")
  ...)
```

Decoded message shapes:

| Type | Map Keys |
|------|----------|
| `AuthenticationOk` | `{"type" "AuthOk"}` |
| `AuthenticationMD5Password` | `{"type" "AuthMD5" "salt" <Bytes>}` |
| `AuthenticationSASL` | `{"type" "AuthSASL" "mechanisms" ["SCRAM-SHA-256"]}` |
| `ReadyForQuery` | `{"type" "ReadyForQuery" "status" "I"}` — I/T/E |
| `RowDescription` | `{"type" "RowDescription" "columns" [{"name" "id" "type-oid" 23 "format" 0} ...]}` |
| `DataRow` | `{"type" "DataRow" "values" ["1" "Alice" nil]}` |
| `CommandComplete` | `{"type" "CommandComplete" "tag" "SELECT 2"}` |
| `ErrorResponse` | `{"type" "ErrorResponse" "severity" "ERROR" "code" "42P01" "message" "..." "detail" "..." "hint" "..."}` |
| `ParseComplete` | `{"type" "ParseComplete"}` |
| `BindComplete` | `{"type" "BindComplete"}` |
| `ParameterDescription` | `{"type" "ParameterDescription" "type-oids" [23 25]}` |
| `ParameterStatus` | `{"type" "ParameterStatus" "name" "server_version" "value" "16.1"}` |
| `BackendKeyData` | `{"type" "BackendKeyData" "pid" 12345 "secret" 67890}` |
| `NoData` | `{"type" "NoData"}` |
| `EmptyQueryResponse` | `{"type" "EmptyQueryResponse"}` |

---

## Layer 3: Connection (airdb-conn.airl)

Owns the TCP socket and drives the protocol state machine.

### Connection State

```airl
;; Connection is an immutable Map threaded through all operations
{
  "socket"      <fd>          ;; TCP file descriptor (runtime opaque)
  "recv-buf"    <Bytes>       ;; Receive buffer (partial messages)
  "status"      "idle"        ;; idle | in-transaction | error | closed
  "pid"         12345         ;; Backend PID (from BackendKeyData)
  "secret"      67890         ;; Backend secret key
  "params"      {}            ;; Server parameters (from ParameterStatus)
  "stmts"       {}            ;; Cached prepared statements: name → RowDescription
}
```

### Connection Functions

```airl
;; Establish TCP connection + complete startup + auth handshake
;; Returns (Ok conn) or (Err reason)
(defn conn-connect
  :sig [(host : String) (port : Int) (user : String)
        (password : String) (database : String) -> _] ...)

;; Send bytes and read until ReadyForQuery, collecting all messages
;; Returns (Ok [conn messages]) or (Err reason)
(defn conn-send-sync
  :sig [(conn : Map) (bytes : Bytes) -> _] ...)

;; Read the next complete backend message from the receive buffer
;; Returns (Ok [conn message]) or (Err "incomplete") or (Err reason)
(defn conn-read-message
  :sig [(conn : Map) -> _] ...)

;; Gracefully close the connection
(defn conn-close
  :sig [(conn : Map) -> _] ...)
```

### Authentication

Supported mechanisms (in preference order):

1. **SCRAM-SHA-256** — RFC 7677. Client sends client-first message, server sends challenge, client sends client-final message with proof. Uses stdlib `sha256` and `hmac` for the SCRAM computation.
2. **MD5** — Legacy. `md5(md5(password + user) + salt)`. Implemented using stdlib `sha256` (MD5 needs a new runtime builtin — see Runtime Changes).
3. **Cleartext** — Sends password as-is. Supported but only over TLS.
4. **Trust** — Server sends AuthenticationOk with no challenge.

---

## Layer 4: Public API (airdb.airl)

The surface that application code uses.

### Connection

```airl
;; Connect to PostgreSQL
;; DSN format: "host=localhost port=5432 user=app password=secret dbname=mydb"
;; Returns (Ok db) or (Err reason)
(defn airdb-connect
  :sig [(dsn : String) -> _] ...)

;; Close connection
(defn airdb-close
  :sig [(db : Map) -> _] ...)
```

### Simple Query

```airl
;; Execute a query and return rows as a List of Maps
;; Column names become Map keys; NULL becomes nil
;; Returns (Ok [db rows]) where rows : List[Map]
(defn airdb-query
  :sig [(db : Map) (sql : String) -> _]
  ;; Example: (airdb-query db "SELECT id, name FROM users LIMIT 10")
  ;; → (Ok [db [{"id" "1" "name" "Alice"} {"id" "2" "name" "Bob"}]])
  ...)

;; Execute a statement with no result rows (INSERT, UPDATE, DELETE, DDL)
;; Returns (Ok [db {"tag" "INSERT 0 1" "rows-affected" 1}]) or (Err reason)
(defn airdb-exec
  :sig [(db : Map) (sql : String) -> _] ...)
```

### Prepared Statements

```airl
;; Prepare a named statement, cache the RowDescription on the connection
;; Returns (Ok [db stmt]) where stmt: {"name" "..." "columns" [...]}
(defn airdb-prepare
  :sig [(db : Map) (name : String) (sql : String) -> _]
  ;; Placeholders: $1, $2, ... (PostgreSQL native syntax)
  ...)

;; Execute a prepared statement with parameter values (all as Strings)
;; Returns (Ok [db rows]) or (Err reason)
(defn airdb-execute
  :sig [(db : Map) (stmt : Map) (params : List) -> _] ...)

;; Prepare and immediately execute (convenience — prepares on first call,
;; reuses on subsequent calls via stmt cache keyed by sql text)
(defn airdb-query-prepared
  :sig [(db : Map) (sql : String) (params : List) -> _] ...)
```

### Transactions

```airl
;; Begin a transaction — returns new conn with status "in-transaction"
(defn airdb-begin
  :sig [(db : Map) -> _] ...)

;; Commit the current transaction
(defn airdb-commit
  :sig [(db : Map) -> _] ...)

;; Rollback the current transaction
(defn airdb-rollback
  :sig [(db : Map) -> _] ...)

;; Execute a function inside a transaction; auto-rollback on Err
;; (fn [db] ...) must return (Ok [db result]) or (Err reason)
(defn airdb-with-transaction
  :sig [(db : Map) (f : _) -> _] ...)
```

### Example Usage

```airl
(let (db-result : _ (airdb-connect "host=localhost port=5432 user=app password=secret dbname=myapp"))
  (match db-result
    (Err reason) (do (print (str "connect failed: " reason "\n")) (exit 1))
    (Ok db)
      (let (rows-result : _ (airdb-query db "SELECT id, name, email FROM users WHERE active = true LIMIT 20"))
        (match rows-result
          (Err reason) (do (print (str "query failed: " reason "\n")) (exit 1))
          (Ok [db rows])
            (do
              (map (fn [row] (print (str (map-get row "name") " — " (map-get row "email") "\n"))) rows)
              (airdb-close db))))))
```

Prepared statement with parameters:

```airl
(let (prep-result : _ (airdb-prepare db "get-user" "SELECT id, name FROM users WHERE id = $1"))
  (match prep-result
    (Err reason) (print (str "prepare failed: " reason "\n"))
    (Ok [db stmt])
      (let (exec-result : _ (airdb-execute db stmt ["42"]))
        (match exec-result
          (Err reason) (print (str "execute failed: " reason "\n"))
          (Ok [db rows])
            (if (= (length rows) 0)
              (print "user not found\n")
              (print (str "found: " (map-get (list-head rows) "name") "\n")))))))
```

Transaction:

```airl
(airdb-with-transaction db
  (fn [db]
    (let (r1 : _ (airdb-exec db "INSERT INTO accounts (user_id, balance) VALUES ($1, $2)" ["7" "1000"]))
      (match r1
        (Err e) (Err e)
        (Ok [db _])
          (airdb-exec db "UPDATE users SET has_account = true WHERE id = $1" ["7"])))))
```

---

## Type Mapping

All values arrive from PostgreSQL as strings (text protocol). AirDB returns them as AIRL Strings. Type coercion helpers are provided as stdlib functions:

```airl
;; (airdb-int row "id")       → Int  (or nil)
;; (airdb-bool row "active")  → Bool (or nil)
;; (airdb-json row "metadata") → Map  (or nil, via json-parse)
(defn airdb-int   :sig [(row : Map) (col : String) -> _] ...)
(defn airdb-bool  :sig [(row : Map) (col : String) -> _] ...)
(defn airdb-json  :sig [(row : Map) (col : String) -> _] ...)
```

NULL columns arrive as `nil` in the row Map (key absent).

| PostgreSQL Type | AIRL Value |
|-----------------|------------|
| INT2, INT4, INT8, OID | String → convert with `airdb-int` |
| FLOAT4, FLOAT8, NUMERIC | String (no native float in AIRL) |
| BOOL | `"t"` or `"f"` → convert with `airdb-bool` |
| TEXT, VARCHAR, CHAR | String |
| BYTEA | String (hex-escaped `\x...`) |
| TIMESTAMP, DATE, TIME | String (ISO 8601) |
| JSON, JSONB | String → convert with `airdb-json` |
| ARRAY | String (PostgreSQL array literal — parse manually) |
| NULL | `nil` (key absent from row Map) |

---

## Phase 3: Connection Pool (airdb-pool.airl)

A connection pool for multi-request workloads (AirGate integration).

### Pool State

```airl
{
  "dsn"       "host=..."
  "min-size"  2
  "max-size"  10
  "idle"      [conn1 conn2]        ;; available connections
  "active"    [conn3]              ;; checked-out connections
  "waiting"   []                  ;; airline channels awaiting a connection
}
```

### Pool Functions

```airl
(defn pool-create
  :sig [(dsn : String) (min-size : Int) (max-size : Int) -> _] ...)

(defn pool-acquire
  :sig [(pool : Map) -> _]
  ;; Returns (Ok [pool conn]) — creates new conn if under max-size,
  ;; blocks via airline future if at max-size
  ...)

(defn pool-release
  :sig [(pool : Map) (conn : Map) -> _]
  ;; Return conn to idle pool; close if conn status is "error"
  ...)

(defn pool-with
  :sig [(pool : Map) (f : _) -> _]
  ;; Acquire, call f, release; rollback on Err
  ...)
```

---

## Runtime Changes Required

One new runtime builtin is needed:

| Builtin | Signature | Purpose |
|---------|-----------|---------|
| `md5` | `(s : String) -> String` | MD5 hash as hex string — needed for MD5 password auth |

SHA-256, HMAC, and PBKDF2 are already in stdlib (for SCRAM-SHA-256). Base64 is already in stdlib. TCP connect/send/recv are already available via airline builtins. No other new builtins are required.

MD5 is needed only for the legacy auth path. If the Postgres server is configured to use SCRAM-SHA-256, `md5` is never called.

---

## File Structure

```
AirDB/
├── src/
│   ├── airdb-wire.airl         Layer 1: wire codec
│   ├── airdb-protocol.airl     Layer 2: message encode/decode
│   ├── airdb-auth.airl         Auth handlers (MD5, SCRAM-SHA-256)
│   ├── airdb-conn.airl         Layer 3: connection state machine
│   ├── airdb-pool.airl         Phase 3: connection pool
│   ├── airdb-query-builder.airl Phase 3: composable query builder
│   └── airdb.airl              Layer 4: public API
├── tests/
│   ├── test-wire.airl          Wire codec encode/decode (no IO)
│   ├── test-protocol.airl      Message framing (no IO)
│   ├── test-auth.airl          Auth computation (no IO — mock challenge)
│   ├── test-conn.airl          Connection integration (requires Postgres)
│   ├── test-query.airl         Query/exec/prepare (requires Postgres)
│   └── test-pool.airl          Pool acquire/release (requires Postgres)
├── examples/
│   ├── hello.airl              SELECT 1 connectivity check
│   ├── crud.airl               INSERT/SELECT/UPDATE/DELETE
│   ├── prepared.airl           Prepared statements
│   └── transactions.airl       Transaction with rollback
├── docs/
│   └── superpowers/specs/
│       └── 2026-04-08-airdb-design.md   (this file)
├── Makefile
├── airtest.sexp
├── VERSION                     0.1.0
├── README.md
└── CLAUDE.md
```

---

## Implementation Phases

### Phase 1 — Connection + Simple Query (MVP)

**Target:** `airdb-connect`, `airdb-query`, `airdb-exec`, `airdb-close`

Deliverables:
- Wire codec for all message types in the startup + simple query path
- MD5 and SCRAM-SHA-256 auth
- `airdb-connect` fully handshakes to `ReadyForQuery`
- `airdb-query` executes SQL and returns rows as `List[Map]`
- `airdb-exec` executes SQL and returns command tag + rows affected
- All wire/protocol tests pass without Postgres (mock byte fixtures)
- Integration tests verified against a real Postgres 16 instance
- Runtime: add `md5` builtin to `airl-rt`

### Phase 2 — Prepared Statements + Transactions

**Target:** `airdb-prepare`, `airdb-execute`, `airdb-query-prepared`, `airdb-begin/commit/rollback`, `airdb-with-transaction`

Deliverables:
- Extended query protocol (Parse/Bind/Describe/Execute/Sync)
- Prepared statement cache on the connection
- Transaction state tracking (`in-transaction`, `error` status)
- `airdb-with-transaction` with automatic rollback on Err
- Type coercion helpers (`airdb-int`, `airdb-bool`, `airdb-json`)

### Phase 3 — Connection Pool + Query Builder

**Target:** `airdb-pool.airl`, `airdb-query-builder.airl`

Deliverables:
- Min/max pool with acquire/release
- Airline future integration (non-blocking acquire when pool exhausted)
- Composable query builder for SELECT/INSERT/UPDATE/DELETE
- AirGate integration example (middleware providing pool from app state)

---

## Ecosystem Dependencies

| Dependency | Used For |
|------------|----------|
| `airline` builtins | `tcp-connect`, `tcp-send`, `tcp-recv`, `tcp-close` |
| `stdlib/sha256.airl` | SCRAM-SHA-256 proof computation |
| `stdlib/hmac.airl` | SCRAM-SHA-256 `HMAC-SHA-256` |
| `stdlib/base64.airl` | SCRAM-SHA-256 message encoding |
| `stdlib/json.airl` | `airdb-json` coercion helper |
| `CairLI` | CLI tool (examples, future `airdb` admin binary) |
| `airline` (pool only) | Futures for blocking `pool-acquire` |

---

## Test Strategy

Unit tests (no Postgres required):
- Wire codec: encode → decode round-trips for every message type
- Protocol: encode frontend messages, decode backend fixtures (raw bytes captured from real Postgres)
- Auth: MD5 hash and SCRAM-SHA-256 proof computed against known test vectors from RFC 7677

Integration tests (requires Postgres 16 in Docker):
- `make postgres-up` starts `postgres:16-alpine` via Docker on port 5432
- `make test-integration` runs connection, query, prepared, transaction suites
- `make postgres-down` stops and removes container

```makefile
postgres-up:
	docker run -d --name airdb-test \
	  -e POSTGRES_USER=test -e POSTGRES_PASSWORD=test -e POSTGRES_DB=testdb \
	  -p 5432:5432 postgres:16-alpine

postgres-down:
	docker rm -f airdb-test
```

---

## Ecosystem Registration

Register AirDB in:
- `repos/AIRL/ECOSYSTEM.md` — Libraries section, after AIReqL
- `airl-workflow/CLAUDE.md` — Project Registry table
- `airl-workflow/scripts/dispatch.sh` — project name `AirDB`

---

## Out of Scope

- MySQL / SQLite / other databases — PostgreSQL wire protocol only in v1
- Binary protocol (format code 1) — text format sufficient for all AIRL types
- `COPY` bulk load protocol — Phase 4 if needed
- SSL/TLS transport for the DB connection — Phase 4; use a TLS proxy (pgbouncer) for now
- Notification channels (`LISTEN`/`NOTIFY`) — Phase 4
- Row streaming / cursor API — rows collected in memory; suits AIRL's immutable model
