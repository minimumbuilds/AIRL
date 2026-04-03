# airq — JSON query tool in pure AIRL

A jq-compatible JSON processor. Parses JSON input, evaluates jq filter expressions, and outputs formatted results. Supports field access, array operations, pipes, builtins, conditionals, and user-defined functions.

## Build

```bash
# Build (from this directory)
make all AIRL_DIR=../..

# Run tests
make test AIRL_DIR=../..
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
  └── cli.airl       CairLI app definition + dispatch
       ├── lexer.airl     jq expression tokenizer
       ├── parser.airl    Recursive descent → AST (nested Maps)
       ├── eval.airl       Tree-walk evaluator (List = generators)
       ├── builtins.airl  Built-in functions (length, keys, select, etc.)
       └── format.airl    Pretty-print / compact JSON output
```

## Key Design

jq is a generator language — each filter produces 0+ outputs. In AIRL: represent as `List[Any]`. Pipe (`|`) = `concatMap` (apply right filter to each element of left result, flatten). Uses `json-parse` from stdlib for input parsing.

## Compilation Order

```
lexer.airl → parser.airl → eval.airl → builtins.airl → format.airl → cli.airl → main.airl
```

## Conventions

- All `defn` functions require `:requires` and `:ensures` contracts
- No loops/mutation — use `fold`, `map`, `filter`, recursion
- Variant constructors uppercase: `(Ok v)`, `(Err e)`, `(Some v)`, `(None)`
- Use `(str ...)` for string concatenation
- AST nodes are Maps: `{"type" "field" "name" "foo"}`, `{"type" "pipe" "left" ... "right" ...}`
