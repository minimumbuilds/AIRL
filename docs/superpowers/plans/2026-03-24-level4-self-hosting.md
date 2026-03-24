# Level 4: Fully Rust-Free Self-Hosting — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a fully Rust-free AIRL compiler that compiles AIRL source to standalone native binaries via C codegen, linked against a pure-C runtime library.

**Architecture:** The bootstrap compiler front-end (lexer, parser, type checker — already written in AIRL) gets a new C codegen backend that emits C source code from IR nodes. A pure-C runtime library (`airl_rt.c`) replaces the Rust `airl-rt` crate with identical ABI. The pipeline is: AIRL source → bootstrap front-end → C source → `cc` → native binary linked against `libairl_rt_c.a`. Three-stage bootstrap verifies fixpoint.

**Tech Stack:** AIRL (bootstrap compiler), C99 (runtime library + generated code), system C compiler (`cc`/`gcc`/`clang`)

---

## Dependency Graph

```
Task 1: C Runtime — value type + memory          (no deps)
Task 2: C Runtime — arithmetic + comparison       (depends on 1)
Task 3: C Runtime — lists                         (depends on 1)
Task 4: C Runtime — strings                       (depends on 1)
Task 5: C Runtime — maps                          (depends on 1)
Task 6: C Runtime — variants, closures, I/O       (depends on 1)
Task 7: C Runtime — build as static library        (depends on 1-6)
Task 8: C codegen backend — literals + arithmetic  (depends on 7)
Task 9: C codegen backend — control flow + let     (depends on 8)
Task 10: C codegen backend — functions + calls     (depends on 9)
Task 11: C codegen backend — lists, variants, match (depends on 10)
Task 12: C codegen backend — closures + lambdas    (depends on 11)
Task 13: C codegen backend — contracts + print     (depends on 12)
Task 14: Bootstrap driver — full pipeline          (depends on 13)
Task 15: Three-stage bootstrap + fixpoint test     (depends on 14)
```

Tasks 1-6 (C runtime) are parallelizable after Task 1 is done.
Tasks 8-13 (C codegen) are sequential — each builds on the previous.

---

## File Structure

### New Files

| File | Responsibility |
|------|---------------|
| `runtime/airl_rt.h` | C runtime header — `RtValue` struct, all `airl_*` function declarations |
| `runtime/airl_rt.c` | C runtime implementation — memory, constructors, arithmetic, comparison, logic |
| `runtime/airl_rt_list.c` | List operations (head, tail, cons, empty, length, at, append, list_new) |
| `runtime/airl_rt_string.c` | String operations (13 builtins: char_at, substring, split, join, etc.) |
| `runtime/airl_rt_map.c` | Map operations (10 builtins: map_new, map_get, map_set, etc.) |
| `runtime/airl_rt_variant.c` | Variant + closure + I/O (make_variant, match_tag, make_closure, call_closure, print) |
| `runtime/Makefile` | Build `libairl_rt_c.a` from all `.c` files |
| `runtime/test_rt.c` | C test harness for the runtime |
| `bootstrap/codegen_c.airl` | C codegen backend — IR nodes → C source code |
| `bootstrap/codegen_c_test.airl` | Tests for C codegen |
| `bootstrap/driver.airl` | Top-level driver: source → lex → parse → typecheck → IR → C codegen → write file |
| `bootstrap/driver_test.airl` | Tests for driver |
| `scripts/bootstrap.sh` | Three-stage bootstrap script |

### Existing Files (no modifications needed)

| File | Role |
|------|------|
| `bootstrap/lexer.airl` | Tokenizer (already complete) |
| `bootstrap/parser.airl` | Parser (already complete) |
| `bootstrap/types.airl` + `bootstrap/typecheck.airl` | Type checker (already complete) |
| `bootstrap/compiler.airl` | AST → IR compiler (already complete) |

---

## Task 1: C Runtime — Value Type and Memory Management

The foundation: `RtValue` tagged union with refcounting.

**Files:**
- Create: `runtime/airl_rt.h`
- Create: `runtime/airl_rt.c`
- Create: `runtime/test_rt.c`
- Create: `runtime/Makefile`

### Design: RtValue Layout

```c
typedef enum {
    RT_NIL, RT_UNIT, RT_INT, RT_FLOAT, RT_BOOL,
    RT_STR, RT_LIST, RT_MAP, RT_VARIANT, RT_CLOSURE
} RtTag;

typedef struct RtValue {
    int rc;          // reference count
    RtTag tag;
    union {
        int64_t i;                          // RT_INT
        double f;                           // RT_FLOAT
        int64_t b;                          // RT_BOOL (0 or 1)
        struct { char* ptr; size_t len; } s; // RT_STR (owned, UTF-8)
        struct { struct RtValue** items; size_t len; size_t cap; } list; // RT_LIST
        struct { /* hash table */ } map;     // RT_MAP
        struct { char* tag; struct RtValue* inner; } variant; // RT_VARIANT
        struct { void* fn_ptr; struct RtValue** captures; size_t cap_count; } closure; // RT_CLOSURE
    } data;
} RtValue;
```

- [ ] **Step 1: Create `runtime/airl_rt.h`** — `RtValue` struct definition, all function declarations matching the Rust `airl-rt` ABI exactly. Every function that exists in the Rust crate must have the same name and signature here. Include guards.

- [ ] **Step 2: Create `runtime/airl_rt.c`** — Implement core functions:
  - `airl_value_retain(RtValue*)` — increment rc
  - `airl_value_release(RtValue*)` — decrement rc, free if zero (recursive for containers)
  - `airl_value_clone(RtValue*)` — deep clone
  - `airl_int(int64_t)` → `RtValue*` — allocate and return
  - `airl_float(double)` → `RtValue*`
  - `airl_bool(int64_t)` → `RtValue*`
  - `airl_nil()` → `RtValue*`
  - `airl_unit()` → `RtValue*`
  - `airl_str(const char* ptr, size_t len)` → `RtValue*` — copies the bytes
  - `airl_as_bool_raw(RtValue*)` → `int64_t` — returns 0 for nil/false/0, 1 otherwise

- [ ] **Step 3: Create `runtime/Makefile`**
  ```makefile
  CC = cc
  CFLAGS = -O2 -Wall -std=c99

  SRCS = airl_rt.c airl_rt_list.c airl_rt_string.c airl_rt_map.c airl_rt_variant.c
  OBJS = $(SRCS:.c=.o)

  libairl_rt_c.a: $(OBJS)
  	ar rcs $@ $^

  test: test_rt
  	./test_rt

  test_rt: test_rt.c $(OBJS)
  	$(CC) $(CFLAGS) -o $@ $^ -lm

  clean:
  	rm -f *.o libairl_rt_c.a test_rt
  ```

- [ ] **Step 4: Create `runtime/test_rt.c`** — Test harness for Task 1 functions:
  ```c
  #include "airl_rt.h"
  #include <assert.h>
  #include <stdio.h>

  void test_int() {
      RtValue* v = airl_int(42);
      assert(v->tag == RT_INT);
      assert(v->data.i == 42);
      assert(v->rc == 1);
      airl_value_release(v);
      printf("PASS: test_int\n");
  }
  // ... tests for float, bool, nil, str, retain/release, as_bool_raw
  ```

- [ ] **Step 5: Build and run tests**
  ```bash
  cd runtime && make test
  ```
  Create stub files for `airl_rt_list.c`, `airl_rt_string.c`, `airl_rt_map.c`, `airl_rt_variant.c` (empty, just `#include "airl_rt.h"`) so the Makefile doesn't fail.

- [ ] **Step 6: Commit**

---

## Task 2: C Runtime — Arithmetic, Comparison, Logic

**Files:**
- Modify: `runtime/airl_rt.c` (add functions)
- Modify: `runtime/test_rt.c` (add tests)

- [ ] **Step 1: Add arithmetic functions** to `airl_rt.c`:
  - `airl_add(a, b)` — int+int, float+float, str+str (concatenation)
  - `airl_sub(a, b)` — int/float only
  - `airl_mul(a, b)` — int/float only
  - `airl_div(a, b)` — int division for ints, float division for floats. Exit on divide-by-zero.
  - `airl_mod(a, b)` — int/float remainder

- [ ] **Step 2: Add comparison functions:**
  - `airl_eq`, `airl_ne`, `airl_lt`, `airl_gt`, `airl_le`, `airl_ge`
  - Compare ints, floats, strings (lexicographic), bools

- [ ] **Step 3: Add logic functions:**
  - `airl_not(a)` → bool
  - `airl_and(a, b)`, `airl_or(a, b)`, `airl_xor(a, b)` → bool

- [ ] **Step 4: Add tests** for all arithmetic/comparison/logic functions including edge cases (string concat, division by zero behavior)

- [ ] **Step 5: Build and run tests**

- [ ] **Step 6: Commit**

---

## Task 3: C Runtime — Lists

**Files:**
- Create: `runtime/airl_rt_list.c`
- Modify: `runtime/test_rt.c`

- [ ] **Step 1: Implement list functions:**
  - `airl_head(list)` → first element (error if empty)
  - `airl_tail(list)` → new list without first element
  - `airl_cons(elem, list)` → new list with elem prepended
  - `airl_empty(list)` → bool (is list empty?)
  - `airl_length(val)` → int (list length, string byte length, map size)
  - `airl_at(list, idx)` → element at index
  - `airl_append(list, elem)` → new list with elem appended
  - `airl_list_new(RtValue** items, size_t count)` → new list from array

  Lists are represented as `RtValue**` arrays (items are refcounted pointers). All operations return NEW lists — the originals are immutable.

- [ ] **Step 2: Add tests** — empty list, singleton, cons/head/tail cycle, append, at with bounds

- [ ] **Step 3: Build and run tests**

- [ ] **Step 4: Commit**

---

## Task 4: C Runtime — Strings

**Files:**
- Create: `runtime/airl_rt_string.c`
- Modify: `runtime/test_rt.c`

- [ ] **Step 1: Implement 13 string builtins:**
  - `airl_char_at(str, idx)` → single-char string (UTF-8 aware)
  - `airl_substring(str, start, end)` → substring (UTF-8 char indices)
  - `airl_chars(str)` → list of single-char strings
  - `airl_split(str, delim)` → list of strings
  - `airl_join(list, sep)` → string
  - `airl_contains(str, sub)` → bool
  - `airl_starts_with(str, prefix)` → bool
  - `airl_ends_with(str, suffix)` → bool
  - `airl_index_of(str, sub)` → int (char index, -1 if not found)
  - `airl_trim(str)` → string (remove leading/trailing whitespace)
  - `airl_to_upper(str)` → string
  - `airl_to_lower(str)` → string
  - `airl_replace(str, old, new)` → string

  **UTF-8 note:** `char_at`, `substring`, `index_of`, and `chars` must iterate by Unicode codepoints, not bytes. Use a helper `utf8_char_len(unsigned char byte)` that returns 1-4 based on leading byte.

- [ ] **Step 2: Add tests** — ASCII and multi-byte UTF-8 strings, empty strings, edge cases

- [ ] **Step 3: Build and run tests**

- [ ] **Step 4: Commit**

---

## Task 5: C Runtime — Maps

**Files:**
- Create: `runtime/airl_rt_map.c`
- Modify: `runtime/test_rt.c`

- [ ] **Step 1: Implement hash table internals** — open-addressing hash map with string keys:
  - `hash_string(const char* key, size_t len)` → `uint64_t` (FNV-1a or similar)
  - `MapEntry { char* key; size_t key_len; RtValue* value; bool occupied; }`
  - Grow at 70% load factor, rehash

- [ ] **Step 2: Implement 10 map builtins:**
  - `airl_map_new()` → empty map
  - `airl_map_from(list)` → map from flat `[k1, v1, k2, v2, ...]`
  - `airl_map_get(map, key)` → value or nil
  - `airl_map_get_or(map, key, default)` → value or default
  - `airl_map_set(map, key, value)` → new map with key set
  - `airl_map_has(map, key)` → bool
  - `airl_map_remove(map, key)` → new map without key
  - `airl_map_keys(map)` → list of strings (sorted)
  - `airl_map_values(map)` → list (in key-sorted order)
  - `airl_map_size(map)` → int

  All mutation operations return NEW maps (COW semantics).

- [ ] **Step 3: Add tests** — create, get, set, remove, keys sorting, from flat list

- [ ] **Step 4: Build and run tests**

- [ ] **Step 5: Commit**

---

## Task 6: C Runtime — Variants, Closures, I/O

**Files:**
- Create: `runtime/airl_rt_variant.c`
- Modify: `runtime/test_rt.c`

- [ ] **Step 1: Implement variant functions:**
  - `airl_make_variant(tag_str, inner)` → variant value
  - `airl_match_tag(val, tag_str)` → inner if tag matches, NULL if not

- [ ] **Step 2: Implement closure functions:**
  - `airl_make_closure(fn_ptr, captures_ptr, count)` → closure value
  - `airl_call_closure(closure, args_ptr, argc)` → result

  Closure calling convention: the function pointer points to a C function whose first N params are captures (prepended) followed by the user args.

  **Important design note:** The closure function pointer signature in C codegen will be `RtValue* (*)(RtValue*, RtValue*, ...)`. Since C doesn't have variadic function pointer calls by arity, use a dispatch-by-arity approach (switch on argc 0-8, cast and call) — same pattern as `dispatch_call` in `bytecode_jit_full.rs`.

- [ ] **Step 3: Implement I/O functions:**
  - `airl_print(val)` → print value to stdout, return nil
  - `airl_print_values(args_ptr, count)` → print multiple values space-separated
  - `airl_type_of(val)` → string ("Int", "Bool", "Str", etc.)
  - `airl_valid(val)` → bool (always true)
  - `airl_read_file(path_str)` → string contents
  - `airl_get_args()` → list of command-line arg strings

- [ ] **Step 4: Implement contract failure:**
  - `airl_jit_contract_fail(kind, fn_idx, clause_idx)` → stores error info, returns 0
  - Thread-local (or global) error cell

- [ ] **Step 5: Implement display for print:** recursive `display_value(RtValue*, FILE*)` that handles all tags:
  - Int → `printf("%lld")`
  - Str → `printf("\"%s\"")`  (with escaping? or raw?)
  - List → `[v1 v2 v3]`
  - Variant → `(Tag inner)`
  - Map → `{k1: v1, k2: v2}`

  Match the Rust `Display` impl for `Value` exactly — the fixture tests compare output strings.

- [ ] **Step 6: Add tests** — variants, closures (create + call), print output capture

- [ ] **Step 7: Build full library and run all tests**
  ```bash
  cd runtime && make clean && make test
  ```

- [ ] **Step 8: Commit**

---

## Task 7: C Runtime — Build and Cross-Validate

**Files:**
- Modify: `runtime/Makefile`

- [ ] **Step 1: Build `libairl_rt_c.a`**
  ```bash
  cd runtime && make libairl_rt_c.a
  ```

- [ ] **Step 2: Cross-validate against Rust `airl-rt`** — Write a test AIRL program that exercises all builtins and produces deterministic output. Compile it two ways:
  ```bash
  # Rust runtime (existing)
  airl compile test_all_builtins.airl -o test_rust
  ./test_rust > output_rust.txt

  # C runtime (new) — manually link for now
  airl compile --emit-obj test_all_builtins.airl  # just the .o
  cc test_all_builtins.o runtime/libairl_rt_c.a -lm -o test_c
  ./test_c > output_c.txt

  diff output_rust.txt output_c.txt  # must be identical
  ```

- [ ] **Step 3: Fix any output differences** — the C runtime must produce byte-identical output to the Rust runtime for all value types.

- [ ] **Step 4: Commit**

---

## Task 8: C Codegen Backend — Literals and Arithmetic

The bootstrap compiler's IR nodes need a C code emitter. Start with the simplest expressions.

**Files:**
- Create: `bootstrap/codegen_c.airl`
- Create: `bootstrap/codegen_c_test.airl`

### Design: Generated C Code Structure

```c
#include "airl_rt.h"

// Forward declarations for all functions
RtValue* my_func(RtValue* p0, RtValue* p1);

// Function definitions
RtValue* my_func(RtValue* p0, RtValue* p1) {
    return airl_add(p0, p1);
}

// Entry point
int main(void) {
    RtValue* __result = /* __main__ body */;
    return 0;
}
```

Every expression becomes a `RtValue*`-typed C expression. IR nodes map to C as:

| IR Node | C Output |
|---------|----------|
| `IRInt(42)` | `airl_int(42)` |
| `IRFloat(3.14)` | `airl_float(3.14)` |
| `IRStr("hello")` | `airl_str("hello", 5)` |
| `IRBool(true)` | `airl_bool(1)` |
| `IRNil` | `airl_nil()` |
| `IRLoad("x")` | `x` |
| `IRCall("+", [a, b])` | `airl_add(emit(a), emit(b))` |

- [ ] **Step 1: Create `bootstrap/codegen_c.airl`** with:
  - `(defn emit-c-expr ...)` — match on IR node tag, return C source string
  - Start with: `IRInt`, `IRFloat`, `IRStr`, `IRBool`, `IRNil`, `IRLoad`
  - `(defn emit-c-call ...)` — map AIRL operator names to C function names (`"+" → "airl_add"`, etc.)

- [ ] **Step 2: Create `bootstrap/codegen_c_test.airl`** — test that `emit-c-expr` produces correct C for each literal type and basic arithmetic

- [ ] **Step 3: Run tests**
  ```bash
  cargo run --release --features jit -- run bootstrap/codegen_c_test.airl
  ```

- [ ] **Step 4: Commit**

---

## Task 9: C Codegen Backend — Control Flow and Let Bindings

**Files:**
- Modify: `bootstrap/codegen_c.airl`
- Modify: `bootstrap/codegen_c_test.airl`

- [ ] **Step 1: Add `IRIf` handling:**
  ```c
  // IRIf([cond, then, else])
  (airl_as_bool_raw(COND) ? THEN : ELSE)
  ```

- [ ] **Step 2: Add `IRLet` handling** — C doesn't have let-expressions, so emit block-scoped variables:
  ```c
  // IRLet([{name: "x", expr: IRInt(5)}], body)
  ({                           // GCC statement-expression
      RtValue* x = airl_int(5);
      BODY;
  })
  ```
  **Alternative if GCC extensions are undesirable:** Use a helper function or nested scope. The GCC statement-expression `({...})` is simpler and supported by GCC and Clang.

- [ ] **Step 3: Add `IRDo` handling** — sequence of expressions, return last:
  ```c
  ({ expr1; expr2; exprN; })
  ```

- [ ] **Step 4: Add tests** — if/else, nested let, do blocks

- [ ] **Step 5: Commit**

---

## Task 10: C Codegen Backend — Functions and Calls

**Files:**
- Modify: `bootstrap/codegen_c.airl`
- Modify: `bootstrap/codegen_c_test.airl`

- [ ] **Step 1: Add `IRFunc` handling** — emit a C function definition:
  ```c
  RtValue* func_name(RtValue* p0, RtValue* p1) {
      return BODY;
  }
  ```
  Parameter names from the IR are mapped to `p0, p1, ...` in C. The codegen maintains a name → C-name mapping.

- [ ] **Step 2: Add `IRCall` handling** — named function calls:
  ```c
  // IRCall("my-func", [a, b])
  my_func(EMIT(a), EMIT(b))
  ```
  Builtin names map to `airl_*` functions. User-defined names are mangled (hyphens → underscores).

- [ ] **Step 3: Add forward declarations** — scan all `IRFunc` nodes first, emit forward declarations before any definitions.

- [ ] **Step 4: Add `emit-c-program`** — top-level function that takes a list of IR nodes and produces a complete `.c` file with `#include`, forward decls, function defs, and `main()`.

- [ ] **Step 5: Add tests** — compile a simple AIRL program to C, verify the C compiles and produces correct output

- [ ] **Step 6: Commit**

---

## Task 11: C Codegen Backend — Lists, Variants, Pattern Matching

**Files:**
- Modify: `bootstrap/codegen_c.airl`
- Modify: `bootstrap/codegen_c_test.airl`

- [ ] **Step 1: Add `IRList` handling:**
  ```c
  // IRList([a, b, c])
  ({
      RtValue* __items[] = { EMIT(a), EMIT(b), EMIT(c) };
      airl_list_new(__items, 3);
  })
  ```

- [ ] **Step 2: Add `IRVariant` handling:**
  ```c
  // IRVariant("Ok", [inner])
  airl_make_variant(airl_str("Ok", 2), EMIT(inner))
  ```

- [ ] **Step 3: Add `IRMatch` handling** — chain of if/else using `airl_match_tag`:
  ```c
  // IRMatch(scrutinee, [(pattern1, body1), (pattern2, body2)])
  ({
      RtValue* __scrutinee = EMIT(scrutinee);
      RtValue* __match_tmp;
      (__match_tmp = airl_match_tag(__scrutinee, airl_str("Ok", 2))) != NULL
          ? ({ RtValue* v = __match_tmp; BODY1; })
          : ({ RtValue* e = __scrutinee; BODY2; })
  })
  ```
  Wildcard patterns just bind the scrutinee. Literal patterns use `airl_eq`.

- [ ] **Step 4: Add `IRTry` handling:**
  ```c
  // IRTry(expr) — unwrap Ok or abort
  ({
      RtValue* __try_val = EMIT(expr);
      RtValue* __try_ok = airl_match_tag(__try_val, airl_str("Ok", 2));
      if (__try_ok == NULL) { fprintf(stderr, "try: not Ok\n"); exit(1); }
      __try_ok;
  })
  ```

- [ ] **Step 5: Add tests** — list creation, variant creation + matching, try unwrap

- [ ] **Step 6: Commit**

---

## Task 12: C Codegen Backend — Closures and Lambdas

**Files:**
- Modify: `bootstrap/codegen_c.airl`
- Modify: `bootstrap/codegen_c_test.airl`

- [ ] **Step 1: Add `IRLambda` handling** — each lambda becomes a static C function with captured variables as leading parameters:
  ```c
  // IRLambda(["x"], body_using_captured_y)
  // Emitted as a named function:
  static RtValue* __lambda_0(RtValue* __cap_y, RtValue* x) {
      return BODY;
  }
  // At the call site:
  airl_make_closure((void*)__lambda_0, captures_array, 1)
  ```
  The codegen must:
  1. Identify free variables in the lambda body (variables not in the parameter list)
  2. Emit a static function with captures as leading params
  3. At the lambda expression site, emit `airl_make_closure` with the captured values

- [ ] **Step 2: Add `IRCallExpr` handling** — calling a value in a register (closure call):
  ```c
  // IRCallExpr(callee_expr, [arg1, arg2])
  ({
      RtValue* __args[] = { EMIT(arg1), EMIT(arg2) };
      airl_call_closure(EMIT(callee), __args, 2);
  })
  ```

- [ ] **Step 3: Handle stdlib higher-order functions** — `map`, `filter`, `fold`, `sort` all pass lambdas. Verify these work end-to-end by compiling a test that uses them.

- [ ] **Step 4: Add tests** — lambda creation, closure capture, higher-order stdlib functions

- [ ] **Step 5: Commit**

---

## Task 13: C Codegen Backend — Contracts, Print, and Polish

**Files:**
- Modify: `bootstrap/codegen_c.airl`
- Modify: `bootstrap/codegen_c_test.airl`

- [ ] **Step 1: Handle contracts in emitted functions** — when a function has `:requires`/`:ensures` clauses, emit assertion checks:
  ```c
  RtValue* my_func(RtValue* p0) {
      // :requires
      if (!airl_as_bool_raw(REQUIRE_EXPR)) {
          airl_jit_contract_fail(0, 0, 0);
          return airl_nil();
      }
      RtValue* __result = BODY;
      // :ensures
      if (!airl_as_bool_raw(ENSURE_EXPR)) {
          airl_jit_contract_fail(1, 0, 0);
          return airl_nil();
      }
      return __result;
  }
  ```

  **Note:** The bootstrap compiler's IR doesn't currently carry contract info. Two options:
  - (a) Add contract IR nodes to the bootstrap compiler
  - (b) Skip contracts in C codegen for now (contracts are already verified by Z3 at compile time)

  Option (b) is simpler for initial self-hosting. Contracts can be added later.

- [ ] **Step 2: Handle variadic print** — `print` with multiple args:
  ```c
  airl_print_values(args_array, count)
  ```

- [ ] **Step 3: Handle name mangling** — AIRL allows hyphens in identifiers (`map-get`, `is-empty-str`). C doesn't. Mangle all names: `-` → `_`, `?` → `_q`, `!` → `_b`.

- [ ] **Step 4: Full integration test** — compile `tests/fixtures/valid/stdlib_math.airl` through the C codegen pipeline and verify output matches the Rust-compiled version:
  ```bash
  # Use bootstrap compiler to emit C
  cargo run --release --features jit -- run bootstrap/driver.airl < tests/fixtures/valid/stdlib_math.airl > /tmp/test.c
  # Compile with C runtime
  cc /tmp/test.c runtime/libairl_rt_c.a -lm -o /tmp/test_c
  # Compare output
  /tmp/test_c > /tmp/output_c.txt
  cargo run --release --features jit -- run tests/fixtures/valid/stdlib_math.airl > /tmp/output_rust.txt
  diff /tmp/output_c.txt /tmp/output_rust.txt
  ```

- [ ] **Step 5: Commit**

---

## Task 14: Bootstrap Driver — Full Pipeline

**Files:**
- Create: `bootstrap/driver.airl`
- Create: `bootstrap/driver_test.airl`

- [ ] **Step 1: Create `bootstrap/driver.airl`** — top-level program that:
  1. Reads source file from command-line args (`get-args`)
  2. Reads file contents (`read-file`)
  3. Lexes → parses → type-checks → IR-compiles (using existing bootstrap modules)
  4. Calls C codegen to produce C source
  5. Writes C source to stdout (or to a file)

  ```lisp
  (defn compile-to-c
    :sig [(source : String) -> String]
    :requires [(valid source)]
    :ensures [(valid result)]
    :body
      (let (tokens : _ (lex-all source))
        (let (ast : _ (parse-all tokens))
          (let (ir : _ (compile-program ast))
            (emit-c-program ir)))))

  ;; Main: read file, compile, print C source
  (let (args : _ (get-args))
    (let (source : _ (read-file (at args 1)))
      (print (compile-to-c source))))
  ```

- [ ] **Step 2: Create `bootstrap/driver_test.airl`** — test the full pipeline on small programs

- [ ] **Step 3: Test end-to-end:**
  ```bash
  # Compile a hello-world AIRL program to C
  cargo run --release --features jit -- run bootstrap/driver.airl -- hello.airl > hello.c
  # Compile the C
  cc hello.c runtime/libairl_rt_c.a -lm -o hello
  # Run it
  ./hello
  ```

- [ ] **Step 4: Commit**

---

## Task 15: Three-Stage Bootstrap and Fixpoint Verification

**Files:**
- Create: `scripts/bootstrap.sh`

- [ ] **Step 1: Create `scripts/bootstrap.sh`:**
  ```bash
  #!/bin/bash
  set -e

  echo "=== Stage 0: Build C runtime ==="
  cd runtime && make clean && make libairl_rt_c.a && cd ..

  echo "=== Stage 1: Compile bootstrap compiler using Rust toolchain ==="
  # Use existing AOT to compile the bootstrap driver + all bootstrap modules
  cargo build --release --features jit,aot
  RUST_MIN_STACK=67108864 target/release/airl-driver compile \
      bootstrap/lexer.airl \
      bootstrap/parser.airl \
      bootstrap/types.airl \
      bootstrap/typecheck.airl \
      bootstrap/compiler.airl \
      bootstrap/codegen_c.airl \
      bootstrap/driver.airl \
      -o stage1
  echo "Stage 1 binary: ./stage1"

  echo "=== Stage 2: Use stage1 to compile itself ==="
  # stage1 reads AIRL, emits C, we compile the C
  ./stage1 compile bootstrap/driver.airl > stage2.c
  cc stage2.c runtime/libairl_rt_c.a -lm -o stage2
  echo "Stage 2 binary: ./stage2"

  echo "=== Stage 3: Use stage2 to compile itself ==="
  ./stage2 compile bootstrap/driver.airl > stage3.c
  cc stage3.c runtime/libairl_rt_c.a -lm -o stage3
  echo "Stage 3 binary: ./stage3"

  echo "=== Fixpoint check ==="
  diff stage2.c stage3.c
  if [ $? -eq 0 ]; then
      echo "FIXPOINT REACHED — stage2.c == stage3.c"
      echo "The AIRL compiler is fully self-hosting."
  else
      echo "FIXPOINT FAILED — stage2.c != stage3.c"
      exit 1
  fi
  ```

- [ ] **Step 2: Run the bootstrap script:**
  ```bash
  chmod +x scripts/bootstrap.sh && ./scripts/bootstrap.sh
  ```

- [ ] **Step 3: If fixpoint fails** — debug by diffing `stage2.c` and `stage3.c`. Common causes:
  - Non-deterministic output (hash map iteration order — use sorted keys)
  - Floating-point formatting differences
  - Lambda counter starting at different values

- [ ] **Step 4: Once fixpoint passes, run the full fixture test suite** through the stage2 compiler to verify correctness:
  ```bash
  for f in tests/fixtures/valid/*.airl; do
      ./stage2 compile "$f" > /tmp/test.c 2>/dev/null || continue
      cc /tmp/test.c runtime/libairl_rt_c.a -lm -o /tmp/test_bin 2>/dev/null || continue
      expected=$(cargo run --release --features jit -- run "$f" 2>/dev/null)
      actual=$(/tmp/test_bin 2>/dev/null)
      [ "$expected" = "$actual" ] && echo "PASS: $(basename $f)" || echo "FAIL: $(basename $f)"
  done
  ```

- [ ] **Step 5: Commit and celebrate**

---

## Summary: Effort Estimates

| Task | Description | Effort |
|------|-------------|--------|
| 1 | C runtime: values + memory | ~3 hours |
| 2 | C runtime: arithmetic + comparison | ~2 hours |
| 3 | C runtime: lists | ~2 hours |
| 4 | C runtime: strings (UTF-8) | ~4 hours |
| 5 | C runtime: maps (hash table) | ~4 hours |
| 6 | C runtime: variants, closures, I/O | ~3 hours |
| 7 | C runtime: build + cross-validate | ~1 hour |
| 8-13 | C codegen backend (6 tasks) | ~8 hours |
| 14 | Bootstrap driver | ~2 hours |
| 15 | Three-stage bootstrap + fixpoint | ~2 hours |
| **Total** | | **~31 hours** |

The C runtime (Tasks 1-7, ~19 hours) is the bulk of the work. The C codegen (Tasks 8-13, ~8 hours) is conceptually simple — it's a pretty-printer from IR to C. The driver and bootstrap (Tasks 14-15, ~4 hours) tie it together.
