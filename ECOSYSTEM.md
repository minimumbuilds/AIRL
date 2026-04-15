# AIRL Ecosystem

## Overview

AIRL (AI Intermediate Representation Language) is a typed, contract-verified programming language designed for AI agent communication. The syntax is S-expressions. The type system enforces linear ownership. Contracts (`:requires`/`:ensures`) are verified statically via Z3 and enforced at runtime.

The compiler is self-hosted: the G3 binary is written in AIRL and compiles itself to produce a bitwise-identical binary (fixpoint verified). The Rust host toolchain provides the runtime library (`libairl_rt.a`), Cranelift code generation, and Z3 integration. Compiled AIRL programs are native x86-64 binaries.

## Core

### AIRL (v0.16.0)

The compiler and runtime. 10-crate Rust workspace + self-hosted bootstrap compiler.

| Component | Description |
|-----------|-------------|
| `airl-syntax` | Lexer, parser, AST, diagnostics |
| `airl-types` | Type checker, linearity, exhaustiveness (symbol interning, COW snapshots) |
| `airl-contracts` | Contract violation types |
| `airl-runtime` | AOT compiler (Cranelift) -- COW fast paths, memory leak fixes, UB aliasing fixes, static singletons, alloc reduction |
| `airl-rt` | Runtime library -- ~150 compiler intrinsics (extern "C") + extern-c stubs for stdlib |
| `airl-codegen` | Cranelift code generation |
| `airl-solver` | Z3 SMT contract verification |
| `airl-agent` | Multi-agent transport (TCP, Unix, stdio) |
| `airl-driver` | CLI: `airl run`, `airl compile`, `airl check`, `airl fmt` -- pipeline optimization |
| `airl-mlir` | Optional GPU/MLIR support |

**Bootstrap:** 30 AIRL files (~27K lines) implementing lexer, parser, bytecode compiler, and G3 driver.

**Stdlib:** 14 modules -- collections, math, result, string, map, set, json, base64, sha256, hmac, pbkdf2, io, path, aircon. 73 functions migrated from Rust builtins to pure AIRL in v0.11.0.

**New runtime builtins:** `dns-resolve`, `icmp-ping` (networking), 8 identity IPC stubs (`whoami`, `id`, `authenticate`, `su`, `sudo`, `useradd`, `userdel`, `usermod`), 7 Canopy terminal extern-c functions, 5 AirCon container IPC stubs (`aircon-create`, `aircon-start`, `aircon-stop`, `aircon-status`, `aircon-list`), `ash-install-sigint`/`ash-sigint-pending` for REPL signal handling (all in `airl-rt`).

**Recent fixes:** AOT arity bug -- use callee's declared arity, not caller's argc.

**Stats:** ~43K Rust LOC (crates/), ~35K AIRL LOC (bootstrap/ + stdlib/), 843 commits, 74 AOT tests. (Counted with `wc -l`.)

**Execution modes:**
- `airl run` -- AOT compile to temp binary, execute, clean up
- `airl compile` -- produce standalone native binary
- `airl check` -- type check and contract verification only

---

## Libraries

All libraries are written in pure AIRL and compiled to native binaries via g3.

### AIRL_castle -- Kafka Client SDK

Full Kafka client implementing the binary TCP protocol from scratch. 13 Kafka APIs, 4 SASL auth mechanisms (PLAIN, SCRAM-SHA-256/512, OAUTHBEARER), 4 compression formats (gzip, snappy, lz4, zstd), consumer groups with rebalancing, and an async producer with request pipelining.

| | |
|---|---|
| **Location** | `../AIRL_castle` |
| **Size** | 8,811 LOC (33 modules), 23 tests, 20 examples |
| **Commits** | 78 |
| **Status** | Functional. Production-quality protocol implementation. |
| **Depends on** | 41 runtime builtins (bytes, crypto, compression, bitwise) |

### AirLift (airl_kafka_cli) -- Kafka CLI

Full-featured Kafka command-line client implementing the binary wire protocol over raw TCP. 20+ Kafka APIs, SASL/PLAIN and SCRAM-SHA-256 auth, gzip/snappy compression, consumer groups, and multiple output formats (text, JSON, CSV). Built with CairLI for subcommand parsing. Named binary: `airlift`.

| | |
|---|---|
| **Location** | `../airl_kafka_cli` |
| **Size** | 4,219 LOC (10 modules), 1,258 LOC tests (335+ assertions) |
| **Commits** | 11 |
| **Status** | Functional. Complete CLI with produce, consume, admin, and group operations. |

### AIReqL (v0.2.0) -- HTTP Client

HTTP client library built from raw TCP. Requests-like API with sessions, cookies, and basic/bearer authentication. Implements HTTP/1.1 request construction and response parsing, URL encoding (RFC 3986), and Set-Cookie handling. Stage 3: HTTP redirect following (301-308), retry with exponential backoff + jitter, connection keep-alive pool.

| | |
|---|---|
| **Location** | `../AIReqL` |
| **Size** | 2,697 LOC (4 modules), 1,264 LOC tests |
| **Commits** | 26 |
| **Status** | Functional. Stage 3 (redirects, retries, keep-alive) complete. |

### Airline -- Async Framework

Seastar-inspired share-nothing async framework. One reactor per CPU core, futures with closure chaining, cross-core message passing, work stealing, and async TCP I/O.

| | |
|---|---|
| **Location** | `../airline` |
| **Size** | 1,217 LOC (7 modules), 484 LOC tests |
| **Commits** | 25 |
| **Status** | Functional. Core async + work stealing complete. Async TCP working. |

### CairLI -- CLI Argument Parser

CLI argument parsing framework with builder-pattern API. Flags (bool, string, int), positional arguments, single and nested subcommands, automatic help text generation.

| | |
|---|---|
| **Location** | `../CairLI` |
| **Size** | 2,197 LOC, 1,465 LOC tests |
| **Commits** | 8 |
| **Status** | Stable (v0.2.0). Feature-complete for current needs. |

### airtools (airlint) -- Static Analysis Linter

Self-hosted linter for AIRL code. Imports the bootstrap compiler's lexer/parser as a library. 14 rules across 4 categories (contracts, style, dead code, ownership). Outputs JSON diagnostics for AI agent consumption. Includes an LSP server for editor integration.

| | |
|---|---|
| **Location** | `../airtools` |
| **Size** | 6,065 LOC (10 modules + LSP server) |
| **Commits** | 8 |
| **Status** | Functional. 14 rules implemented, LSP server scaffold complete. |

### AIRLchart -- Code Visualization

Generates Graphviz DOT call graphs and type dependency diagrams from AIRL source code. Parses AIRL files using the bootstrap lexer/parser, extracts function definitions, call edges, type references, and import relationships, then emits DOT output with clustered subgraphs per file. Supports recursive import following.

| | |
|---|---|
| **Location** | `../AIRLchart` |
| **Size** | 1,313 LOC (38 functions) |
| **Commits** | 10 |
| **Status** | Functional. Produces DOT/SVG output for AIRL codebases. |
| **Output** | Graphviz DOT (convertible to SVG/PNG/PDF) |

### airlhttp -- HTTP/1.1 Server Library

HTTP/1.1 server library with routing, middleware pipeline, and TLS support. Built on the Airline async framework. Includes a CLI harness for standalone serving.

| | |
|---|---|
| **Location** | `../airlhttp` |
| **Size** | 2,230 LOC (9 modules) |
| **Commits** | 3 |
| **Status** | Functional. Phase 1 complete (server, router, middleware, TLS). |

### AirGate (v0.2.0) -- Web Application Framework

Full-featured web framework built on airlhttp. Routing with path parameters and wildcards, middleware pipeline (logger, CORS, body-parser, auth), mustache-like templates, HMAC-signed sessions, static file serving, and structured error handling. Phase 2: WebSocket support (RFC 6455 frames), form validation, CSRF protection, flash messages, response compression (gzip), structured JSON logging.

| | |
|---|---|
| **Location** | `../AirGate` |
| **Size** | 1,890 LOC (10 modules), 865 LOC tests (10 test suites) |
| **Commits** | 7 |
| **Status** | Functional. Phase 2 complete (WebSocket, validation, CSRF, flash messages, compression, logging). |
| **Depends on** | airlhttp, airline, stdlib (json, string, collections, hmac) |

### AirParse -- Multi-Format Parser Library

Parsers and serializers for JSON (extends stdlib), YAML, TOML, and HTML (DOM tree with CSS selectors). Unified `airparse-parse`/`airparse-stringify`/`airparse-convert` API with round-trip support across all formats.

| | |
|---|---|
| **Location** | `../AirParse` |
| **Size** | 1,784 LOC (6 modules), 776 LOC tests (121 tests across 4 test suites) |
| **Commits** | 6 |
| **Status** | Functional. All four formats implemented with round-trip support. |
| **Depends on** | stdlib (json, string, collections) |

### AirTraffic -- MCP Server Framework

Model Context Protocol (MCP) server framework for AIRL. Enables building MCP-compatible tool and prompt servers that compile to native binaries. Role-based tool filtering, prompt registration, JSON-RPC 2.0, JSON Schema validation. Includes a workflow server for multi-agent coordination.

| | |
|---|---|
| **Location** | `../AirTraffic` |
| **Size** | 1,358 LOC (4 modules + workflow server) |
| **Commits** | 5 |
| **Status** | Functional. Tool and prompt support merged to main. |

### AirLog -- Structured Logging Framework

Structured logging library for AIRL applications. Level-based filtering (debug, info, warn, error), key-value structured fields, pluggable outputs (console, file), and JSON log format support. Includes file rotation via the file output module.

| | |
|---|---|
| **Location** | `../AirLog` |
| **Size** | 649 LOC (2 modules + tests + examples) |
| **Commits** | 1 |
| **Status** | Functional. Core API and file output complete. |
| **Depends on** | stdlib (json, string, io) |

### AirNexus -- AI Agent Framework

Multi-provider AI agent framework for building LLM-powered applications in AIRL. Unified API across OpenAI, Anthropic, Gemini, and Ollama. Streaming support, structured JSON output with schema validation, tool use with call tracking, orchestration primitives (pipeline and parallel composition), and pluggable output handlers (text, JSON, callback). Built on AIReqL for HTTP transport and AirLog for structured logging.

| | |
|---|---|
| **Location** | `../airnexus` |
| **Size** | ~1,630 LOC (10 core modules, 4 provider modules, 6 examples, 5 test suites) |
| **Commits** | 7 |
| **Version** | 0.1.0 |
| **Status** | Functional. All providers, streaming, tool use, and orchestration complete. 33 tests passing. |
| **Depends on** | AIReqL (HTTP transport), AirLog, AirTraffic, stdlib (json, base64) |

### mynameisAIRL -- MCP Prompt Server + Code Indexer

MCP prompt server that serves AIRL-LLM-Guide.md to LLMs as a `teach_airl` prompt, plus the AirMunch code indexer providing 10 MCP tools: `index_project`, `file_tree`, `file_outline`, `get_symbol`, `search_symbols`, `get_content`, `repo_outline`, `find_callers`, `dependency_graph`, `blast_radius`. Built on the AirTraffic framework. Supports CLI, environment variable, and Docker volume-mount guide path resolution. Stdio transport.

| | |
|---|---|
| **Location** | `servers/mynameisairl` (inside AIRL repo) |
| **Size** | 1,963 LOC |
| **Commits** | 7 |
| **Status** | Functional. Native and Docker builds. AirMunch indexer with 10 MCP tools. |

### Canopy -- Algebraic TUI Framework

Terminal UI framework built on a single premise: the UI is data. Scenes are S-expression lists, layout is a pure fold, diffing is pattern matching, rendering produces ANSI escape sequences. No components, no virtual DOM, no mutable state -- just pure functions transforming data. Supports boxes (column/row layout, grow weights, padding, borders, overflow scroll), styled text, and spacers. Channel-based event coordination for concurrent input and resize handling. Phase B adds interactive terminal applications: key/mouse event dispatch, focus management, text input widgets, scroll views, and a reactive app-loop with model-update-view architecture.

| | |
|---|---|
| **Location** | `../canopy` |
| **Size** | 1,384 LOC (7 src modules, 6 test suites, 2 demos) |
| **Commits** | 4 |
| **Status** | Functional. Phase A+B complete (layout, rendering, diffing, interactive TUI). |
| **Depends on** | 7 extern-c terminal I/O functions in AIRL runtime, stdlib (string, collections, sha256) |

### AirMux -- Terminal Multiplexer

tmux-like terminal multiplexer for the AIRL ecosystem. Manages named sessions of windows and panes, each backed by a PTY pair running ash. Built on Canopy TUI. Supports horizontal/vertical pane splitting, window tabs, detach/attach. Prefix key: Ctrl-a.

| | |
|---|---|
| **Location** | `../AirMux` |
| **Size** | ~1,100 LOC (3 AIRL modules + C runtime, 1 test suite) |
| **Version** | 0.1.0 |
| **Status** | PoC complete. 64 tests passing. Binary builds on Linux. |
| **Depends on** | Canopy (TUI framework), AIRL stdlib (map, string, json), libutil (openpty) |

### AirSeal -- JWT Library

Pure-AIRL JWT signing/verification. HS256 implementation with base64url encoding. Depends on stdlib `sha256`, `hmac-sha256`, `base64`, `json`.

| | |
|---|---|
| **Location** | `../AirSeal` |
| **Size** | 678 LOC (2 modules), 308 LOC tests |
| **Commits** | 3 |
| **Status** | Functional. JWT sign/verify with HS256. |

### AirPost -- SMTP Client

Pure-AIRL SMTP client. RFC 5321 + STARTTLS + AUTH PLAIN/LOGIN, MIME multipart message construction.

| | |
|---|---|
| **Location** | `../AirPost` |
| **Size** | 1,172 LOC (3 modules), 247 LOC tests |
| **Commits** | 3 |
| **Status** | Functional. SMTP protocol, MIME multipart, and auth complete. |

### AirHangar -- S3 Object Storage Client

Pure-AIRL S3-compatible client built on AIReqL. SigV4 signer; supports MinIO and AWS S3. Presigned GET/PUT.

| | |
|---|---|
| **Location** | `../AirHangar` |
| **Size** | 1,210 LOC (2 modules), 414 LOC tests |
| **Commits** | 3 |
| **Status** | Functional. SigV4 signing and S3 operations complete. |

### AirFlux -- Schema Migration Runner

Flyway/golang-migrate-style forward-only migrations over Postgres. Depends on AirDB, CairLI, AirLog.

| | |
|---|---|
| **Location** | `../AirFlux` |
| **Size** | 546 LOC (1 module), 215 LOC tests |
| **Commits** | 3 |
| **Status** | Functional. Forward-only migration execution. |

---

## Applications

### platy-airl -- HR/Compliance Platform

Application built on the AIRL ecosystem. Consumes AirGate, AirDB, AirHangar, AirSeal, AirPost, AirFlux, AirLog. PDF framework storage, RAG search over handbooks, acknowledgment tracking, background jobs, and JWT-authenticated web endpoints.

| | |
|---|---|
| **Location** | `../platy-airl` |
| **Size** | 4,004 LOC (16 modules), 1,849 LOC tests |
| **Commits** | 48 |
| **Status** | Functional. Full HR platform with auth, search, ingestion, job queue, and dashboard. |

---

## Persistence

### AirWire -- Wire Protocol Primitives

Shared library for binary protocol encoding and SCRAM-SHA-256 authentication. Two modules: `wire-binary` provides offset-threading big-endian int codecs (`encode/decode-int8/16/32/64`, `encode/decode-cstring`) and the `make-decoded` Map constructor used throughout the ecosystem. `wire-scram` provides the 10 pure SCRAM-SHA-256/512 functions from RFC 5802 — key derivation, proof computation, nonce exchange — shared between the Kafka SDK and PostgreSQL client without duplication.

| | |
|---|---|
| **Location** | `../AirWire` |
| **Size** | ~600 LOC (2 modules), 38 assertions (unit tests) |
| **Version** | 0.2.0 |
| **Status** | Functional. Wire-binary and wire-scram modules complete. |
| **Depends on** | stdlib (bytes, base64, hmac, pbkdf2, sha256) |

### AirDB -- PostgreSQL Client SDK

PostgreSQL wire protocol v3 client implemented in pure AIRL. Full 5-layer stack: pg-wire (frame encode/decode, `pg-peek-message`), pg-protocol (all 10 frontend encoders + 15 backend decoders), pg-auth (SCRAM-SHA-256 via AirWire — md5 deliberately excluded), pg-conn (TCP state machine, startup handshake, ReadyForQuery), airdb (public API). Phases 1 and 2 complete: `airdb-connect`, `airdb-query`, `airdb-exec`, `airdb-prepare`, `airdb-execute`, transactions (`airdb-begin/commit/rollback`, `airdb-with-transaction`). Extended query protocol with `$1/$2` parameter binding. Coercion helpers: `airdb-int`, `airdb-bool`, `airdb-json`.

Note: server must be configured for `scram-sha-256` auth (PostgreSQL 14+ default). md5 auth is not implemented (deprecated in PG 14, disabled by default in PG 17).

| | |
|---|---|
| **Location** | `../AirDB` |
| **Size** | ~1,000 LOC (5 modules), 75 assertions (unit tests) |
| **Version** | 0.2.0 |
| **Status** | Functional. Phases 1+2 complete. Unit tests pass. Integration tests require PostgreSQL 16. |
| **Depends on** | AirWire, airline, stdlib (bytes, string, json) |

---

## Tooling

### airlDelivery (aird) -- Package Manager

Package manager and ecosystem tooling for AIRL. Subcommands: build, test, docs (ECOSYSTEM.md generator, README validator). Built with CairLI for CLI parsing. Named binary: `aird`.

| | |
|---|---|
| **Location** | `../airlDelivery` |
| **Size** | 4,196 LOC |
| **Commits** | 9 |
| **Status** | Functional. Build, test, and docs subcommands implemented. |

### airtest -- Test Runner

Test discovery, compilation, execution, and JSON reporting for AIRL projects. Discovers `*_test.airl` files, compiles each with g3, runs them, and aggregates results into structured JSON output.

| | |
|---|---|
| **Location** | `../airtest` |
| **Size** | 891 LOC |
| **Commits** | 3 |
| **Status** | Functional. Discovery, compilation, execution, JSON output. |

---

## Testing and Benchmarks

### AIRL_bench -- Code Generation Benchmark

Measures how well language models generate AIRL code. 100 tasks across 4 difficulty tiers (stdlib, composition, recursion, contracts). Compares AIRL against Python and C on correctness, token efficiency, and execution speed. The harness itself is written in AIRL.

| | |
|---|---|
| **Location** | `../AIRL_bench` |
| **Size** | 847 LOC harness, 100 task specifications |
| **Commits** | 27 |
| **Key results** | qwen3-coder: 100/100 (100%). AIRL uses 0.43× the completion tokens of Python. |

**Progression:** 44% (no guide) -> 68% (+ guide) -> 80% (+ few-shot) -> 100% (v0.6.0+ stdlib improvements).

### kafka_sdk_bench -- Kafka SDK Performance Benchmark

Benchmarks AIRL_castle's Kafka producer against Confluent's librdkafka (Python wrapper). Measures sync, batch, and async producer throughput on a single localhost broker. Includes detailed performance analysis identifying per-component costs (TCP, encoding, cluster management) and root causes for performance gaps.

| | |
|---|---|
| **Location** | `../kafka_sdk_bench` |
| **Languages** | AIRL (producer) + Python/librdkafka (baseline) |
| **Size** | ~1,073 LOC (AIRL benchmark + Python baseline + orchestrator + analysis) |
| **Key results** | Sync: AIRL 5.9K vs Confluent 7.9K msg/s (75%). Batch: 46K msg/s. Root cause: per-byte value boxing. |
| **Status** | Functional. 14 optimization specs documenting improvement roadmap. |

---

## Operating System

### AIRLOS -- Capability-Based Microkernel

32-bit x86 microkernel with per-process page tables, capability-based security (12 capability bits), synchronous IPC (256-byte messages), async notifications, shared memory, and lwIP TCP/IP networking. Boots via Multiboot/GRUB on QEMU. Includes an embedded AIRL S-expression evaluator for kernel-side policy evaluation, and a TCP agent server with HMAC-SHA256 authentication.

**GUI foundation:** VESA framebuffer with bitmap font rendering (Spec 30), PS/2 mouse driver with IRQ 12 packet decode and event broadcast (Spec 31), window server with compositor (Spec 32), and display protocol for client-server rendering (Spec 33).

**VFS and exec:** VFS-based `/bin/` directory populated from ramdisk ELFs at boot (Spec 34). `SYS_EXEC_BUF` syscall for loading and executing binaries from VFS.

**Networking:** 12 C runtime builtins, DNS resolution + ICMP ping in net service, SSH server with command interpreter.

**AirCon container stack (v0.2.x):** Four-phase container support:
- Phase 1 (v0.2.2): Task groups with resource limits (`sc_group_create/spawn/destroy/setlimit`)
- Phase 2 (v0.2.3): VFS mount namespaces per container
- Phase 3 (v0.2.4): `.aircon` binary image format (CON1 magic, S-expression manifest, ELF layers) + container service IPC at 0xC00 (`CONTAINER_CREATE/START/STOP/STATUS/LIST`)
- Phase 4 (v0.2.5): Network namespaces — `net_ns_manager` service, auto-assigned 10.0.1.x/24 IPs, IPC at 0xD00 (`NET_NS_CREATE/DESTROY/BIND/QUERY`), `CAP_NET_RAW` required for mutating ops

| | |
|---|---|
| **Location** | `../AIRLOS` |
| **Language** | C (freestanding, gnu99), x86 assembly |
| **Size** | ~36,100 LOC kernel + drivers + user-space (excluding vendored lwIP) |
| **Commits** | 190 |
| **Version** | 0.2.6 |
| **Status** | Functional prototype. Security hardening complete (Spec 00 fixed). 35 design specs. CI via GitHub Actions. Full container lifecycle (create/start/stop/status/list) with network isolation. |

### airshell -- Interactive Shell

zsh-compatible interactive shell targeting AIRLOS. REPL with line editing, command history, 13+ built-in commands, environment variable expansion, S-expression config file (`.ashrc`), and configurable prompt. Cross-compiles to AIRLOS via `make airlos`. Also runs natively on Linux. Full scripting support: if/for/while/case/function, trap handlers (EXIT/INT/ERR/DEBUG), `$@`/`$*` support, POSIX dispatch order, ping/host builtins.

**Job control:** Background execution (`&`), `jobs`/`fg`/`bg` builtins, job completion tracking in the REPL loop.

**Identity:** `whoami`, `id`, `groups` builtins. Standalone programs: `passwd`, `su`, `sudo`, `useradd`, `userdel`, `usermod` (in `programs/`, compiled separately).

**Container management:** `aircon` standalone program with `create/start/stop/status/list/help` sub-commands. On AIRLOS: IPC to container service at 0xC00. On Linux: stub with clear "not available" message.

**Signal handling:** Ctrl-C (SIGINT) installs a non-terminating handler at REPL startup; `read_line` detects interruption and re-prompts with exit code 130. Running INT trap if registered. Cooperative SIGINT check in long-running builtins (ping).

**Ping:** Per-probe output lines (`64 bytes from IP: icmp_seq=N ttl=T time=X ms`, `Request timeout for icmp_seq N`, `Destination Host Unreachable`). Linux stub exits cleanly after first probe. RTT/packet-loss summary on completion.

| | |
|---|---|
| **Location** | `../airshell` |
| **Size** | ~4,600 LOC (11 modules + 8 standalone programs), ~1,200 LOC tests |
| **Commits** | 28 |
| **Status** | Functional. Linux and AIRLOS targets. Full scripting, POSIX dispatch, job control, identity management, container CLI, and SIGINT handling. |

### AirLock -- SSH Client

SSH client for AIRLOS. Implements the SSH-2 protocol from scratch: key exchange (Curve25519/ECDH), host key verification (Ed25519), authentication (none probe, password, publickey), channel multiplexing, and interactive terminal sessions. Built with CairLI for CLI argument parsing.

| | |
|---|---|
| **Location** | `../AirLock` |
| **Size** | 2,607 LOC (9 modules) |
| **Commits** | 3 |
| **Status** | Functional. SSH-2 protocol complete. Interactive terminal sessions. |
| **Depends on** | stdlib (bytes, crypto, string), CairLI |

---

## Ecosystem Stats

| Project | Language | LOC | Commits | Status |
|---------|----------|-----|---------|--------|
| AIRL | Rust + AIRL | ~78,000 | 843 | v0.16.0, self-hosted |
| AIRLOS | C + asm | 36,100 | 190 | v0.2.6, Prototype |
| AIRL_castle | AIRL | 8,811 | 78 | Functional |
| airtools | AIRL | 6,065 | 8 | Functional |
| AirLift | AIRL | 4,219 | 11 | Functional |
| airshell | AIRL | 4,600 | 28 | Functional |
| airlDelivery | AIRL | 4,196 | 9 | Functional |
| AIReqL | AIRL | 2,697 | 26 | v0.2.0 |
| airlhttp | AIRL | 2,230 | 3 | Functional |
| CairLI | AIRL | 2,197 | 8 | Stable (v0.2.0) |
| mynameisAIRL | AIRL | 1,963 | 7 | Functional |
| AirGate | AIRL | 1,890 | 7 | v0.2.0 |
| AirParse | AIRL | 1,784 | 6 | Functional |
| Canopy | AIRL | 1,384 | 4 | Functional |
| AirTraffic | AIRL | 1,358 | 5 | Functional |
| AIRLchart | AIRL | 1,313 | 10 | Functional |
| kafka_sdk_bench | AIRL + Python | 1,073 | 3 | Functional |
| airline | AIRL | 1,217 | 25 | Functional |
| airtest | AIRL | 891 | 3 | Functional |
| AIRL_bench | AIRL | 847 | 27 | Functional |
| AirLog | AIRL | 649 | 1 | Functional |
| AirLock | AIRL | 2,607 | 3 | Functional |
| AirWire | AIRL | 600 | 3 | v0.2.0 |
| AirDB | AIRL | 1,000 | 3 | v0.2.0 |
| AirMux | AIRL + C | 1,100 | 2 | v0.1.0 PoC |
| AirNexus | AIRL | 1,630 | 7 | v0.1.0 Functional |
| AirSeal | AIRL | 678 | 3 | Functional |
| AirPost | AIRL | 1,172 | 3 | Functional |
| AirHangar | AIRL | 1,210 | 3 | Functional |
| AirFlux | AIRL | 546 | 3 | Functional |
| platy-airl | AIRL | 4,004 | 48 | Functional |
| **Total** | | **~176,000** | **~1,121** | |

## Building

All AIRL ecosystem projects require the g3 compiler and `libairl_rt.a` from the core AIRL repo:

```bash
# Build the host toolchain (one-time, ~5-15 min)
cd AIRL
cargo build --release --features aot

# Build g3 self-hosted compiler (one-time, ~1 min)
bash scripts/build-g3.sh

# Compile any AIRL project
export AIRL_STDLIB=./stdlib
./g3 -- file1.airl file2.airl -o binary
```

Individual projects may require additional link flags (`-lm -lpthread -ldl`) when using g3 directly. See each project's README for specific build instructions.
