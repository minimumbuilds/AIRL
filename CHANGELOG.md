# Changelog

All notable changes to AIRL are documented in this file.

## [Unreleased]

### Added
- TLA+ formal model of AIRL thread/channel primitives (`0ab47ce`)
- TLA+ formal verification report across ecosystem (`1492605`)
- COW optimization for `map-set` and `map-remove` builtins (`2643b4f`)
- `define` form — lightweight function definition without contracts (`b5c7952`)
- `shell-exec` enhanced to return `{stdout, stderr, exit-code}` map (`70ba9ab`)
- 12 new builtins — char classification, radix parsing, utilities (`5a005ef`)
- `--target` flag for i386 cross-compilation (`2033a41`)
- G3 build caching with timestamped binaries (`67c3503`)

### Fixed
- Channel recv race condition, close errors, and close drain (`52a3bf2`)

### Removed
- JIT backend removed; consolidated to AOT-only (`9ae31e3`)
- Dead `http-request` builtin stub removed (`87a0fee`)

## [0.9.0] — 2026-03-30

### Added
- **macOS/ARM64 AOT support** — Cranelift patches and target settings (`d767afe`)
- **Dynamic Z3 linking on macOS**, static on Linux (`4dc7681`)
- macOS build instructions and fresh-checkout build order (`ff8f137`)
- stdin/stderr builtins: `read-line`, `read-stdin`, `eprint`, `eprintln` (`ff8980c`)
- Thin LTO and release profile optimizations (`6988062`)

### Fixed
- Replace all 107 `.unwrap()` calls in bytecode VM with `expect`/proper errors (`250802a`)
- Z3 solver diagnostic for result-referencing postconditions (`b48c9e6`)
- G3 shows help when invoked without arguments (`cbec7b1`)

### Reverted
- All allocator tier experiments (mimalloc, slab pool, Arc keys) reverted due to regressions (`f463c34`)

### Changed
- Version bumped to 0.9.0 (`fc137e7`)
- License updated to PolyForm Noncommercial 1.0.0 (`ddb6192`)
