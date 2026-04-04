# AIRL Ecosystem

## Overview

AIRL (AI Intermediate Representation Language) is a typed, contract-verified programming language designed for AI agent communication. The syntax is S-expressions. The type system enforces linear ownership. Contracts (`:requires`/`:ensures`) are verified statically via Z3 and enforced at runtime.

The compiler is self-hosted: the G3 binary is written in AIRL and compiles itself to produce a bitwise-identical binary (fixpoint verified). The Rust host toolchain provides the runtime library (`libairl_rt.a`), Cranelift code generation, and Z3 integration. Compiled AIRL programs are native x86-64 binaries.

## Core

### AIRL (v0.11.0)

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

**Stdlib:** 13 modules -- collections, math, result, string, map, set, json, base64, sha256, hmac, pbkdf2, io, path. 73 functions migrated from Rust builtins to pure AIRL in v0.11.0.

**New runtime builtins:** `dns-resolve`, `icmp-ping` (networking, in `airl-rt`).

**Stats:** 35K Rust LOC, 44K AIRL LOC, 577 commits, 690 unit tests, 76 AOT tests.

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
| **Size** | 2,005 LOC (10 modules + LSP server) |
| **Commits** | 5 |
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

### mynameisAIRL -- MCP Prompt Server + Code Indexer

MCP prompt server that serves AIRL-LLM-Guide.md to LLMs as a `teach_airl` prompt, plus the AirMunch code indexer providing 10 MCP tools: `index_project`, `file_tree`, `file_outline`, `get_symbol`, `search_symbols`, `get_content`, `repo_outline`, `find_callers`, `dependency_graph`, `blast_radius`. Built on the AirTraffic framework. Supports CLI, environment variable, and Docker volume-mount guide path resolution. Stdio transport.

| | |
|---|---|
| **Location** | `servers/mynameisairl` (inside AIRL repo) |
| **Size** | 1,963 LOC |
| **Commits** | 7 |
| **Status** | Functional. Native and Docker builds. AirMunch indexer with 10 MCP tools. |

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
| **Key results** | qwen3-coder: 100/100 (100%). AIRL is 2.7x more token-efficient than Python. |

**Progression:** 44% (no guide) -> 68% (+ guide) -> 80% (+ few-shot) -> 100% (v0.6.0+ stdlib improvements).

### kafka_sdk_bench -- Kafka SDK Performance Benchmark

Benchmarks AIRL_castle's Kafka producer against Confluent's librdkafka (Python wrapper). Measures sync, batch, and async producer throughput on a single localhost broker. Includes detailed performance analysis identifying per-component costs (TCP, encoding, cluster management) and root causes for performance gaps.

| | |
|---|---|
| **Location** | `../kafka_sdk_bench` |
| **Languages** | AIRL (producer) + Python/librdkafka (baseline) |
| **Size** | ~750 LOC (AIRL benchmark + Python baseline + orchestrator + analysis) |
| **Key results** | Sync: AIRL 5.9K vs Confluent 7.9K msg/s (75%). Batch: 46K msg/s. Root cause: per-byte value boxing. |
| **Status** | Functional. 14 optimization specs documenting improvement roadmap. |

---

## Operating System

### AIRLOS -- Capability-Based Microkernel

32-bit x86 microkernel with per-process page tables, capability-based security (12 capability bits), synchronous IPC (256-byte messages), async notifications, shared memory, and lwIP TCP/IP networking. Boots via Multiboot/GRUB on QEMU. Includes an embedded AIRL S-expression evaluator for kernel-side policy evaluation, and a TCP agent server with HMAC-SHA256 authentication. 12 new C runtime builtins, DNS resolution + ICMP ping in net service, keyboard debug traces, ash boot fix (keyboard service ID caching).

| | |
|---|---|
| **Location** | `../AIRLOS` |
| **Language** | C (freestanding, gnu99), x86 assembly |
| **Size** | ~34,300 LOC kernel + drivers + user-space (excluding vendored lwIP) |
| **Commits** | 177 |
| **Status** | Functional prototype. Security hardening complete (Spec 00 fixed). 19 design specs. CI via GitHub Actions. |

### airshell -- Interactive Shell

zsh-compatible interactive shell targeting AIRLOS. REPL with line editing, command history, 13 built-in commands, environment variable expansion, S-expression config file (`.ashrc`), and configurable prompt. Cross-compiles to AIRLOS via `make airlos`. Also runs natively on Linux. Full scripting support: if/for/while/case/function, trap handlers (EXIT/INT/ERR/DEBUG), `$@`/`$*` support, POSIX dispatch order, ping/host builtins.

| | |
|---|---|
| **Location** | `../airshell` |
| **Size** | 2,864 LOC (11 modules), 929 LOC tests |
| **Commits** | 19 |
| **Status** | Functional. Linux and AIRLOS targets. Full scripting and POSIX dispatch. |

---

## Ecosystem Stats

| Project | Language | LOC | Commits | Status |
|---------|----------|-----|---------|--------|
| AIRL | Rust + AIRL | 78,659 | 577 | v0.11.0, self-hosted |
| AIRLOS | C + asm | 34,300 | 177 | Prototype |
| AIRL_castle | AIRL | 8,811 | 78 | Functional |
| AirLift | AIRL | 4,219 | 11 | Functional |
| airlDelivery | AIRL | 4,196 | 9 | Functional |
| airshell | AIRL | 2,864 | 19 | Functional |
| AIReqL | AIRL | 2,697 | 26 | v0.2.0 |
| airlhttp | AIRL | 2,230 | 3 | Functional |
| CairLI | AIRL | 2,197 | 8 | Stable (v0.2.0) |
| airtools | AIRL | 2,005 | 5 | Functional |
| mynameisAIRL | AIRL | 1,963 | 7 | Functional |
| AirGate | AIRL | 1,890 | 7 | v0.2.0 |
| AirParse | AIRL | 1,784 | 6 | Functional |
| AirTraffic | AIRL | 1,358 | 5 | Functional |
| AIRLchart | AIRL | 1,313 | 10 | Functional |
| kafka_sdk_bench | AIRL + Python | 1,281 | 3 | Functional |
| airline | AIRL | 1,217 | 25 | Functional |
| airtest | AIRL | 891 | 3 | Functional |
| AIRL_bench | AIRL | 847 | 27 | Functional |
| **Total** | | **~154,722** | **1,006** | |

## Building

All AIRL ecosystem projects require the g3 compiler and `libairl_rt.a` from the core AIRL repo:

```bash
# Build the host toolchain (one-time, ~5-15 min)
cd AIRL
cargo build --release --features aot

# Build g3 self-hosted compiler (one-time, ~23 min)
bash scripts/build-g3.sh

# Compile any AIRL project
export AIRL_STDLIB=./stdlib
./g3 -- file1.airl file2.airl -o binary
```

Individual projects may require additional link flags (`-lm -lpthread -ldl`) when using g3 directly. See each project's README for specific build instructions.
