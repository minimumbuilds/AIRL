# File I/O Builtins — Design Spec

**Date:** 2026-03-22
**Purpose:** Add file I/O builtins to AIRL, prerequisite for self-hosting (Phase 3).

## Builtins

| Builtin | Signature | Returns | Behavior |
|---------|-----------|---------|----------|
| `read-file` | `(read-file path)` | `String` or error | Read entire file as UTF-8 string |
| `write-file` | `(write-file path content)` | `true` or error | Write string to file (creates/overwrites) |
| `file-exists?` | `(file-exists? path)` | `Bool` | Check if path exists |

## Sandboxing

All paths resolved relative to CWD. Reject any path that:
- Contains `..`
- Starts with `/`

Return `RuntimeError::Custom` with a clear message on rejection.

## Error Handling

`read-file` and `write-file` return `RuntimeError::Custom` on failure (file not found, permission denied, not valid UTF-8). Follows the same pattern as `spawn-agent` and `send`.

## Implementation Location

- **Builtins:** `crates/airl-runtime/src/builtins.rs` — register in `Builtins::new()`
- **Symbol registration:** `crates/airl-runtime/src/eval.rs` — add to `register_builtin_symbols`
- **Type checker:** `crates/airl-types/src/checker.rs` — add to known symbols

## Tests

- Unit tests in `builtins.rs` using temp files (read, write, exists, sandboxing rejection)
- Fixture in `tests/fixtures/valid/` that uses `file-exists?`

## Non-Goals

- Directory listing, path manipulation, append mode, delete — add later if needed
- Binary file support — UTF-8 strings only
- Async I/O — synchronous is fine for a compiler
