# airgrep — Recursive grep in pure AIRL

A ripgrep-compatible recursive search tool. Searches directories recursively, respects .gitignore, supports regex patterns, ANSI color output, context lines, glob filtering, and file type filtering.

## Build

```bash
# Build (from this directory)
make all AIRL_DIR=../..

# Run tests
make test AIRL_DIR=../..

# Or manually
cd ../../ && AIRL_STDLIB=./stdlib \
  ./g3 -- ~/repos/CairLI/src/cairli.airl \
  stdlib/io.airl stdlib/string.airl stdlib/path.airl stdlib/map.airl \
  commands/airgrep/src/glob.airl \
  commands/airgrep/src/ignore.airl \
  commands/airgrep/src/types.airl \
  commands/airgrep/src/walk.airl \
  commands/airgrep/src/search.airl \
  commands/airgrep/src/format.airl \
  commands/airgrep/src/cli.airl \
  commands/airgrep/src/main.airl \
  -o commands/airgrep/build/airgrep
```

## g3 Quirks

- **No `defn main`:** Top-level expressions in main.airl are the entry point.
- **Use `(print (str ... "\n"))` not `(println ...)`**
- **`match` only works on Variants** — use `if`/`=` for strings
- **No `\x1b`** — use `(char-from-code 27)` for ESC
- **No `define`** — use `defn` with contracts
- **Source file order matters** — last file is entry point

## Architecture

```
main.airl       Entry point — top-level cairli-run-or-die
  └── cli.airl     CairLI app definition + dispatch
       ├── walk.airl     Recursive dir traversal with filtering
       │    ├── ignore.airl   .gitignore parser
       │    ├── glob.airl      Glob → regex conversion
       │    └── types.airl     File type definitions
       ├── search.airl   Line-by-line regex search + context
       └── format.airl   ANSI color output formatting
```

## Compilation Order

```
glob.airl → ignore.airl → types.airl → walk.airl → search.airl → format.airl → cli.airl → main.airl
```

## Conventions

- All `defn` functions require `:requires` and `:ensures` contracts
- No loops/mutation — use `fold`, `map`, `filter`, recursion
- Variant constructors uppercase: `(Ok v)`, `(Err e)`, `(Some v)`, `(None)`
- Use `(str ...)` for string concatenation
- ANSI escape via `(char-from-code 27)`, NOT `\x1b` or `\e`
