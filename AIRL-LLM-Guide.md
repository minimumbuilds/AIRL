# AIRL Reference Guide (v1.1.0)

> A complete reference for writing AIRL (AI Intermediate Representation Language) programs.
> AIRL is an S-expression language designed for AI systems, featuring mandatory contracts,
> linear ownership, tensor operations, and multi-agent orchestration.
>
> **v1.1.0** introduced the stdlib migration: ~150 functions remain as compiler intrinsics
> (always available), while 73 functions moved to the pure-AIRL standard library (auto-loaded
> from `stdlib/`). New features: `extern-c` declarations and byte-array intrinsics.

---

## 1. Quick Start

AIRL programs are sequences of S-expressions. Every expression returns a value.

```lisp
;; hello.airl — A minimal program
(print "Hello, World!")

(defn greet
  :sig [(name : String) -> String]
  :ensures [(valid result)]
  :body (do
    (print "Hello," name)
    name))

(greet "AI")
```

Run with `cargo run -- run hello.airl`. Type-check and verify with `cargo run -- check hello.airl`.

---

## CRITICAL: What AIRL Does NOT Have

**AIRL is a purely functional language. The following constructs DO NOT EXIST and will cause parse or runtime errors if used:**

| Do NOT write | AIRL has no... | Instead use |
|---|---|---|
| `while`, `for`, `loop` | Loops of any kind | `fold`, `map`, `filter`, or recursion |
| `set!`, `var`, `mut x =` | Variable mutation/reassignment | New `let` bindings (immutable) |
| `return` | Early return | The body expression IS the return value |
| `class`, `def`, `import` | OOP or Python-style definitions | `defn` for functions, `deftype` for types |
| `ord`, `chr` | Character-to-integer (Python-style) | `char-code` / `char-from-code` for char↔integer conversion |
| `begin`, `progn` | Other sequencing forms | `do` for sequencing |
| `+=`, `-=`, `++`, `--` | Compound assignment | Compute new value in `let` or `fold` |

**If you find yourself wanting a loop, STOP and use one of these patterns:**

```lisp
;; Accumulate a value over a range → fold
(fold (fn [acc _] (* acc base)) 1 (range 0 exponent))

;; Transform each element → map
(map (fn [x] (* x 2)) [1 2 3])

;; Keep matching elements → filter
(filter (fn [x] (> x 3)) [1 2 3 4 5])

;; Conditional recursion → defn that calls itself
(defn factorial
  :sig [(n : i64) -> i64]
  :requires [(>= n 0)]
  :ensures [(> result 0)]
  :body (if (= n 0) 1 (* n (factorial (- n 1)))))
```

---

## 2. Core Syntax

### S-Expressions

Everything is an S-expression: `(operator arg1 arg2 ...)`. Parentheses delimit calls. Square brackets `[...]` are used for lists, parameter lists, and contract clause lists.

### Comments

```lisp
;; This is a line comment. There are no block comments.
```

### Literals

| Literal | Example | Runtime type |
|---------|---------|-------------|
| Integer | `42`, `-7`, `0` | `Int` (i64) |
| Float | `3.14`, `-0.5`, `1.0` | `Float` (f64) |
| String | `"hello"` | `Str` |
| Boolean | `true`, `false` | `Bool` |
| Nil | `nil` | `Nil` |
| Keyword | `:name`, `:max` | `Str` (evaluates to the string `":name"`) |
| List | `[1 2 3]` | `List` |

**Important:** Integer literals produce `Int` (signed i64). There is no unsigned literal syntax — unsigned values come from function return types. Float and integer types do not auto-coerce; `(+ 1 1.0)` is a type error.

### Symbols

Symbols are identifiers: `x`, `my-function`, `tensor.add`. Hyphens are allowed in names. Symbols starting with an uppercase letter are variant constructors: `Ok`, `Err`, `Some`, `None`.

### S-Expression Nesting and Parenthesis Counting

Multi-binding `let` is the **preferred** style — multiple bindings in one `let`, with the body as the last expression:

```lisp
;; PREFERRED — multi-binding let: 3 bindings, 1 closing paren
(let (a : i64 1)
     (b : i64 2)
     (c : i64 (+ a b))
  (print c))
```

**Common mistake — closing `let` before its body:**
```lisp
;; WRONG — the let is closed before the body
(let (x : i64 (+ a b)))    ;; ← extra ) closes the let with no body
  (print x))                ;; ← this is NOT inside the let

;; CORRECT
(let (x : i64 (+ a b))     ;; ← no extra ), body follows
  (print x))                ;; ← this IS the let body
```

**`if` has exactly 3 subforms** — condition, then-branch, else-branch:
```lisp
;; WRONG — 4 subforms (let after else looks like a 4th form)
(if (= n 0) [] (let (x : i64 5)) x)

;; CORRECT — wrap in do if you need multiple expressions
(if (= n 0) [] (let (x : i64 5) x))
```

---

## 3. Function Definitions

### Template

```lisp
(defn function-name
  :sig [(param1 : Type1) (param2 : Type2) -> ReturnType]
  :intent "optional human-readable description"
  :requires [precondition1 precondition2 ...]
  :ensures [postcondition1 postcondition2 ...]
  :invariant [invariant1 invariant2 ...]
  :body expression)
```

### Required Fields

- **`:sig`** — Parameter list and return type. Always required.
- **`:body`** — The function body expression. Always required.
- **At least one contract** — Either `:requires` or `:ensures` must be present (or both).

### Optional Fields

- **`:intent`** — Human-readable string describing purpose.
- **`:invariant`** — Checked after body evaluation, before `:ensures`.
- **`:execute-on`** — `cpu`, `gpu`, `any`, or an agent name.
- **`:priority`** — `critical`, `high`, `normal`, `low`.

### Parameter Syntax

Parameters are written as `(name : Type)` inside the `:sig` bracket list:

```lisp
:sig [(x : i32) (y : i32) -> i32]
```

With ownership annotations, the annotation comes before the name:

```lisp
:sig [(own x : i32) (ref y : i32) -> i32]
```

### Examples

**Simple function:**
```lisp
(defn add-one
  :sig [(x : i32) -> i32]
  :requires [(valid x)]
  :ensures [(= result (+ x 1))]
  :body (+ x 1))
```

**Function with preconditions:**
```lisp
(defn clamp
  :sig [(val : i64) (lo : i64) (hi : i64) -> i64]
  :requires [(<= lo hi)]
  :ensures [(>= result lo) (<= result hi)]
  :body (if (< val lo) lo (if (> val hi) hi val)))
```

**Recursive function:**
```lisp
(defn factorial
  :sig [(n : i64) -> i64]
  :requires [(>= n 0) (<= n 20)]
  :ensures [(>= result 1)]
  :body (if (<= n 1) 1 (* n (factorial (- n 1)))))
```

---

## 4. Types

### Primitive Types

| Type name | Description | Runtime value |
|-----------|-------------|--------------|
| `i8`, `i16`, `i32`, `i64` | Signed integers | `Int` (all stored as i64) |
| `u8`, `u16`, `u32`, `u64` | Unsigned integers | `UInt` (all stored as u64) |
| `f16`, `f32`, `f64`, `bf16` | Floating point | `Float` (all stored as f64) |
| `bool` | Boolean | `Bool` |
| `String` | UTF-8 string | `Str` |
| `Nat` | Natural number (type-level) | `Int` at runtime |

### Compound Types

| Type syntax | Description |
|-------------|-------------|
| `tensor` | Tensor value (unparameterized at runtime) |
| `fn` | Function/lambda type |
| `List` | Ordered list of values |
| `Result[T, E]` | Sum type with `(Ok val)` and `(Err reason)` variants |

### Copy Semantics

All primitive types except `String` support copy semantics. Tensors, functions, and named types are not copyable — they follow move semantics by default.

---

## 5. Control Flow

### If Expression

```lisp
(if condition then-branch else-branch)
```

**Both branches are required.** There is no `if` without `else`.

```lisp
(if (< x 0) (* x -1) x)  ;; absolute value
```

### Let Binding

Bind one or more values, then evaluate a body expression:

```lisp
(let (x : i32 5) (+ x 1))               ;; single binding → 6

(let (x : i32 5) (y : i32 10) (+ x y))  ;; multiple bindings → 15
```

Each binding has the form `(name : Type value)`. The type annotation is required.

### Do Block

Evaluate multiple expressions sequentially, return the last:

```lisp
(do
  (print "step 1")
  (print "step 2")
  42)  ;; returns 42
```

### Match Expression

Pattern match on a value:

```lisp
(match expr
  pattern1 body1
  pattern2 body2
  ...)
```

Arms are **pattern/body pairs** (not wrapped in extra parens). There must be an even number of items after the scrutinee.

```lisp
(match (safe-divide 10 3)
  (Ok v)  (print "Result:" v)
  (Err e) (print "Error:" e))
```

### Pattern Types

| Pattern | Example | Matches |
|---------|---------|---------|
| Binding | `x` | Anything, binds to `x` |
| Wildcard | `_` | Anything, discards |
| Literal | `42`, `"hi"`, `true`, `nil` | Exact value |
| Variant | `(Ok v)` | Variant constructor with nested pattern |
| Nested | `(Ok (Some v))` | Arbitrarily nested variants |

### Try Expression

Unwrap an `(Ok val)` to `val`, or propagate `(Err ...)` as a runtime error:

```lisp
(try (safe-divide 10 2))  ;; returns 5
(try (safe-divide 10 0))  ;; runtime error: "division by zero"
```

---

## 6. Intrinsics (Always Available)

As of v1.1.0, AIRL has ~150 **compiler intrinsics** — built into the runtime as `extern "C"` functions, always available without any imports. These include arithmetic, comparison, logic, type conversion, collections (core), string (core), float math, byte encoding, TCP, compression, regex, concurrency, and tensors.

An additional 73 functions live in the **standard library** (`stdlib/`), written in pure AIRL and auto-loaded as a prelude. These include string helpers, JSON, base64, crypto (SHA-256, HMAC, PBKDF2), file I/O, directory I/O, system functions, paths, and extended map/set/collection operations. See sections 11–15 for stdlib documentation.

### Arithmetic

All arithmetic operators take exactly **2 arguments** of the **same type**. No implicit coercion.

| Operator | Types | Notes |
|----------|-------|-------|
| `+` | Int, UInt, Float, Str | String concatenation with `+` |
| `-` | Int, UInt, Float | |
| `*` | Int, UInt, Float | |
| `/` | Int, UInt, Float | Integer division for Int/UInt. Raises `DivisionByZero` on 0 (Int/UInt only) |
| `%` | Int, UInt, Float | Remainder. Raises `DivisionByZero` on 0 (Int/UInt only) |

```lisp
(+ 3 4)          ;; → 7
(* 2.5 4.0)      ;; → 10.0
(+ "hello" " ")  ;; → "hello "
(/ 10 3)         ;; → 3 (integer division)
```

### Comparison

All take 2 arguments, return `Bool`. Work on Int, UInt, Float, and String.

| Operator | Meaning |
|----------|---------|
| `=` | Equal |
| `!=` | Not equal |
| `<` | Less than |
| `>` | Greater than |
| `<=` | Less than or equal |
| `>=` | Greater than or equal |

### Logic

| Operator | Arity | Description |
|----------|-------|-------------|
| `and` | **2 only** | Logical AND (both must be Bool) |
| `or` | **2 only** | Logical OR |
| `not` | 1 | Logical NOT |
| `xor` | 2 | Logical XOR |

**`and` and `or` take exactly 2 arguments.** They are NOT variadic. To combine multiple conditions, nest them:

```lisp
;; WRONG — or takes 2 args, not 5
(or (= x 1) (= x 2) (= x 3) (= x 4) (= x 5))

;; CORRECT — nest binary or calls
(or (= x 1) (or (= x 2) (or (= x 3) (or (= x 4) (= x 5)))))
```

### Collections

| Function | Signature | Description |
|----------|-----------|-------------|
| `length` | `List → Int` or `Str → Int` | Length of list or string |
| `at` | `(at list index)` → element | Index into a list (0-based). Raises on out of bounds |
| `append` | `(append list element)` → List | Return new list with element added at end |
| `head` | `(head list)` → element | First element of list. Errors on empty list |
| `tail` | `(tail list)` → List | All elements except the first. Errors on empty list |
| `empty?` | `(empty? list)` → Bool | **List only.** Returns `true` if list is empty. For strings use `(= s "")` |
| `cons` | `(cons element list)` → List | Prepend element to front of list |
| `at-or` | `(at-or list index default)` → element | Index with default (returns default on out of bounds) |
| `set-at` | `(set-at list index value)` → List | Return new list with element at index replaced |
| `list-contains?` | `(list-contains? list elem)` → Bool | Check if list contains element |
| `reverse` | `(reverse list)` → List | Reverse a list (native builtin) |
| `concat` | `(concat list1 list2)` → List | Concatenate two lists (native builtin) |
| `flatten` | `(flatten list-of-lists)` → List | Flatten one level of nesting (native builtin) |
| `range` | `(range start end)` → List | Integers `[start, end)` (native builtin) |

```lisp
(length [1 2 3])      ;; → 3
(at [10 20 30] 1)     ;; → 20
(append [1 2] 3)      ;; → [1 2 3]
(head [10 20 30])     ;; → 10
(tail [10 20 30])     ;; → [20 30]
(empty? [])           ;; → true
(cons 0 [1 2 3])      ;; → [0 1 2 3]
(= "" "")             ;; → true (check empty string this way, NOT with empty?)
```

**Functions that do NOT exist in AIRL** — do not use these:

`nil?`, `null?`, `list`, `try`, `catch`, `throw`, `ord`, `chr`, `char-at-int`, `string-ref`, `number->string`, `string->number`, `typeof`, `instanceof`, `require`, `import`

Use `char-code` / `char-from-code` for character↔integer conversion. Use `take` / `drop` from stdlib for list slicing. If you need key-value associations, use `Map` (not lists of pairs). If you need to parse a number from a string, use `string-to-int` or `string-to-float`. If you need to check string emptiness, use `(= s "")`. For character count (Unicode-aware), use `(char-count s)` instead of `(length s)` (which returns byte length).

### Utility

| Function | Arity | Description |
|----------|-------|-------------|
| `str` | variadic | Concatenate all arguments into one String. Strings are included as-is (no quotes); all other types are auto-coerced via Display. `(str "count: " 42 " done")` → `"count: 42 done"` |
| `print` | variadic | Print all arguments to stdout, returns Unit |
| `println` | variadic | Print all arguments to stdout followed by a newline, returns Unit |
| `type-of` | 1 | Returns type name as String (e.g., `"Int"`, `"Bool"`) |
| `shape` | 1 | Returns tensor shape as List of Int |
| `valid` | 1 | Always returns `true`. Used as a minimal contract guard |
| `format` | variadic | Format string with `{}` placeholders: `(format "Hello, {}!" "world")` → `"Hello, world!"` |
| `exit` | 1 | Exit the program with the given integer exit code |
| `char-count` | 1 | Returns Unicode character count of a string (not byte length) |

### Type Conversion

| Function | Signature | Description |
|----------|-----------|-------------|
| `int-to-string` | `(int-to-string n)` → Str | Convert integer to string |
| `float-to-string` | `(float-to-string f)` → Str | Convert float to string |
| `string-to-int` | `(string-to-int s)` → Int | Parse string as integer (panics on invalid input) |
| `string-to-float` | `(string-to-float s)` → Float | Parse string as float (panics on invalid input) |
| `char-code` | `(char-code s)` → Int | First character's Unicode code point |
| `char-from-code` | `(char-from-code n)` → Str | Unicode code point to single-character string |

### Error Handling

| Function | Signature | Description |
|----------|-----------|-------------|
| `panic` | `(panic message)` | Halt execution with error message |
| `assert` | `(assert condition)` | Panic if condition is `false` |

### Float Math

All float math builtins operate on `f64` values. Integer arguments are promoted to `f64`.

| Function | Description |
|----------|-------------|
| `sqrt` | Square root |
| `sin`, `cos`, `tan` | Trigonometric functions (radians) |
| `log` | Natural logarithm |
| `exp` | e^x |
| `floor`, `ceil`, `round` | Rounding operations |
| `float-to-int` | Truncate f64 to i64 |
| `int-to-float` | Promote i64 to f64 |
| `infinity` | Returns positive infinity |
| `nan` | Returns NaN |
| `is-nan?` | Test if value is NaN |
| `is-infinite?` | Test if value is infinite |

### System

| Function | Signature | Description |
|----------|-----------|-------------|
| `shell-exec` | `(shell-exec cmd)` → Str | Execute a shell command, return stdout |
| `time-now` | `(time-now)` → Int | Current time as epoch milliseconds |
| `sleep` | `(sleep ms)` → Nil | Pause execution for `ms` milliseconds |
| `format-time` | `(format-time epoch-ms fmt)` → Str | Format epoch millis with strftime pattern |
| `getenv` | `(getenv name)` → Str/Nil | Read environment variable |
| `get-args` | `(get-args)` → List | Command-line arguments as list of strings |

### JSON (stdlib — auto-loaded from `stdlib/json.airl`)

JSON functions were compiler builtins prior to v1.1.0 and are now implemented in pure AIRL. Auto-loaded in the prelude.

| Function | Signature | Description |
|----------|-----------|-------------|
| `json-parse` | `(json-parse str)` → any | Parse JSON string into AIRL value |
| `json-stringify` | `(json-stringify val)` → Str | Serialize AIRL value to JSON string |

**HTTP:** Use the AIReqL library (`../AIReqL`). See the AIRL-Header.md for the AIReqL API.

### File I/O (stdlib — auto-loaded from `stdlib/io.airl`)

These functions were compiler builtins prior to v1.1.0 and are now implemented in pure AIRL. Auto-loaded in the prelude.

| Function | Signature | Description |
|----------|-----------|-------------|
| `read-file` | `(read-file path)` → Str | Read entire file as string |
| `write-file` | `(write-file path content)` → Nil | Write string to file (overwrites) |
| `append-file` | `(append-file path content)` → Nil | Append string to file |
| `file-exists?` | `(file-exists? path)` → Bool | Check if file exists |
| `read-lines` | `(read-lines path)` → List | Read file as list of lines |
| `delete-file` | `(delete-file path)` → Nil | Delete a file |
| `delete-dir` | `(delete-dir path)` → Nil | Delete a directory |
| `rename-file` | `(rename-file old new)` → Nil | Rename/move a file |
| `create-dir` | `(create-dir path)` → Nil | Create a directory (recursive) |
| `read-dir` | `(read-dir path)` → List | List directory entries |
| `file-size` | `(file-size path)` → Int | File size in bytes |
| `is-dir?` | `(is-dir? path)` → Bool | Check if path is a directory |

### Path (stdlib — auto-loaded from `stdlib/path.airl`)

Path functions were compiler builtins prior to v1.1.0 and are now implemented in pure AIRL. Auto-loaded in the prelude.

| Function | Signature | Description |
|----------|-----------|-------------|
| `path-join` | `(path-join parts...)` → Str | Join path components |
| `path-parent` | `(path-parent path)` → Str | Parent directory |
| `path-filename` | `(path-filename path)` → Str | Filename component |
| `path-extension` | `(path-extension path)` → Str | File extension |
| `is-absolute?` | `(is-absolute? path)` → Bool | Is the path absolute? |

### Regex (v0.3.0)

| Function | Signature | Description |
|----------|-----------|-------------|
| `regex-match` | `(regex-match pattern str)` → Str/Nil | First match, or nil |
| `regex-find-all` | `(regex-find-all pattern str)` → List | All matches |
| `regex-replace` | `(regex-replace pattern str replacement)` → Str | Replace all matches |
| `regex-split` | `(regex-split pattern str)` → List | Split by pattern |

### Crypto (stdlib — auto-loaded from `stdlib/sha256.airl`, `stdlib/hmac.airl`, `stdlib/pbkdf2.airl`, `stdlib/base64.airl`)

These functions were compiler builtins prior to v1.1.0 and are now implemented in pure AIRL. They are auto-loaded in the prelude — no imports needed.

| Function | Signature | Description |
|----------|-----------|-------------|
| `sha256` | `(sha256 str)` → Str | SHA-256 hash (hex string) |
| `sha512` | `(sha512 str)` → Str | SHA-512 hash (hex string) |
| `hmac-sha256` | `(hmac-sha256 key message)` → Str | HMAC-SHA256 (hex string) |
| `hmac-sha512` | `(hmac-sha512 key message)` → Str | HMAC-SHA512 (hex string) |
| `sha256-bytes` | `(sha256-bytes buf)` → IntList | Raw 32-byte hash of IntList |
| `sha512-bytes` | `(sha512-bytes buf)` → IntList | Raw 64-byte hash of IntList |
| `hmac-sha256-bytes` | `(hmac-sha256-bytes key data)` → IntList | Raw HMAC of IntList inputs |
| `hmac-sha512-bytes` | `(hmac-sha512-bytes key data)` → IntList | Raw HMAC of IntList inputs |
| `pbkdf2-sha256` | `(pbkdf2-sha256 password salt iterations key-length)` → IntList | Key derivation |
| `pbkdf2-sha512` | `(pbkdf2-sha512 password salt iterations key-length)` → IntList | Key derivation |
| `base64-encode` | `(base64-encode str)` → Str | Base64 encode |
| `base64-decode` | `(base64-decode str)` → Str | Base64 decode |
| `base64-encode-bytes` | `(base64-encode-bytes buf)` → Str | Encode IntList to base64 string |
| `base64-decode-bytes` | `(base64-decode-bytes s)` → IntList | Decode base64 string to IntList |
| `random-bytes` | `(random-bytes n)` → List | List of n random byte values (0-255) — intrinsic |

### Byte-Array Intrinsics (v1.1.0)

Low-level byte-array operations for mutable, fixed-size byte buffers. These complement the IntList-based byte functions.

| Function | Signature | Description |
|----------|-----------|-------------|
| `bytes-alloc` | `(bytes-alloc n)` → ByteArray | Allocate a zero-filled byte array of size n |
| `bytes-get` | `(bytes-get buf index)` → Int | Read byte at index (0-255) |
| `bytes-set!` | `(bytes-set! buf index value)` → Nil | Write byte at index (mutates in place) |
| `bytes-length` | `(bytes-length buf)` → Int | Length of byte array |

### Byte Encoding (v0.4.0)

Binary data is represented as `IntList` (list of integers 0-255). All integer encoding uses big-endian byte order.

| Function | Signature | Description |
|----------|-----------|-------------|
| `bytes-from-int16` | `(bytes-from-int16 n)` → IntList | Encode i16 as 2 bytes (big-endian) |
| `bytes-from-int32` | `(bytes-from-int32 n)` → IntList | Encode i32 as 4 bytes (big-endian) |
| `bytes-from-int64` | `(bytes-from-int64 n)` → IntList | Encode i64 as 8 bytes (big-endian) |
| `bytes-to-int16` | `(bytes-to-int16 buf offset)` → Int | Decode i16 from byte list at offset |
| `bytes-to-int32` | `(bytes-to-int32 buf offset)` → Int | Decode i32 from byte list at offset |
| `bytes-to-int64` | `(bytes-to-int64 buf offset)` → Int | Decode i64 from byte list at offset |
| `bytes-from-string` | `(bytes-from-string s)` → IntList | UTF-8 encode string to bytes |
| `bytes-to-string` | `(bytes-to-string buf offset len)` → Str | UTF-8 decode bytes to string |
| `bytes-concat` | `(bytes-concat a b)` → IntList | Concatenate two byte lists |
| `bytes-concat-all` | `(bytes-concat-all parts)` → IntList | Concatenate List[IntList] in one O(n) pass |
| `bytes-slice` | `(bytes-slice buf offset len)` → IntList | Extract slice with bounds check |
| `crc32c` | `(crc32c buf)` → Int | CRC32C (Castagnoli) checksum |

### TCP Sockets (v0.4.0)

Handle-based TCP networking. Connections are managed via integer handles. All operations return `Result`.

| Function | Signature | Description |
|----------|-----------|-------------|
| `tcp-listen` | `(tcp-listen port backlog)` → Result[Int, Str] | Bind + listen, returns server handle |
| `tcp-accept` | `(tcp-accept handle)` → Result[Int, Str] | Blocking accept, returns connection handle |
| `tcp-connect` | `(tcp-connect host port)` → Result[Int, Str] | Connect to host:port, returns handle |
| `tcp-close` | `(tcp-close handle)` → Result[Nil, Str] | Close a connection or listener |
| `tcp-send` | `(tcp-send handle data)` → Result[Int, Str] | Send byte list, returns bytes sent |
| `tcp-recv` | `(tcp-recv handle max-bytes)` → Result[IntList, Str] | Receive up to max-bytes |
| `tcp-recv-exact` | `(tcp-recv-exact handle n)` → Result[IntList, Str] | Receive exactly n bytes or error |
| `tcp-set-timeout` | `(tcp-set-timeout handle ms)` → Result[Nil, Str] | Set read/write timeout (ms ≤ 0 = none) |

```lisp
;; TCP client example
(let (conn : _ (tcp-connect "127.0.0.1" 8080))
  (match conn
    (Ok handle) (do
      (tcp-send handle (bytes-from-string "GET / HTTP/1.0\r\n\r\n"))
      (let (response : _ (tcp-recv handle 4096))
        (match response
          (Ok data) (print (bytes-to-string data 0 (length data)))
          (Err e) (print "recv error:" e)))
      (tcp-close handle))
    (Err e) (print "connect error:" e)))
```

### Concurrency (v0.5.0)

Thread-per-task model with message-passing channels. No shared mutable state.

| Function | Signature | Description |
|----------|-----------|-------------|
| `thread-spawn` | `(thread-spawn closure)` → Int | Spawn OS thread running 0-arg closure, returns handle |
| `thread-join` | `(thread-join handle)` → Result | Block until done. Ok(value) or Err(error-msg) |
| `thread-set-affinity` | `(thread-set-affinity core-id)` → Result | Pin calling thread to CPU core (Linux only) |
| `cpu-count` | `(cpu-count)` → Int | Available parallelism (logical CPU count) |
| `channel-new` | `(channel-new)` → [Int Int] | Create unbounded channel, returns [sender receiver] handles |
| `channel-send` | `(channel-send tx value)` → Result | Send value. Err if channel closed |
| `channel-recv` | `(channel-recv rx)` → Result | Blocking receive. Err if channel closed |
| `channel-recv-timeout` | `(channel-recv-timeout rx ms)` → Result | Receive with timeout. ms=0 is non-blocking |
| `channel-drain` | `(channel-drain rx)` → List | Drain all available messages without blocking |
| `channel-close` | `(channel-close handle)` → Bool | Close sender or receiver endpoint |

```lisp
;; Spawn a thread, capture variables from enclosing scope
(let (x : i64 10)
  (let (h : i64 (thread-spawn (fn [] (+ x 5))))
    (match (thread-join h)
      (Ok v) v       ;; 15
      (Err e) -1)))

;; Producer/consumer via channels
(let (ch : List (channel-new))
  (let (tx : i64 (at ch 0))
    (let (rx : i64 (at ch 1))
      (do
        (thread-spawn (fn []
          (do (channel-send tx "hello") (channel-close tx))))
        (match (channel-recv rx)
          (Ok v) v       ;; "hello"
          (Err _) "failed"))))))
```

### Tensor Operations

All tensors are f32 internally. Shapes are specified as bracket lists of integers.

#### Creation

| Function | Args | Description |
|----------|------|-------------|
| `tensor.zeros` | `(tensor.zeros [dim1 dim2 ...])` | All-zeros tensor |
| `tensor.ones` | `(tensor.ones [dim1 dim2 ...])` | All-ones tensor |
| `tensor.rand` | `(tensor.rand [dim1 dim2 ...] seed)` | Random tensor (seed is an integer) |
| `tensor.identity` | `(tensor.identity n)` | n×n identity matrix |

```lisp
(tensor.zeros [3 3])     ;; 3x3 zero matrix
(tensor.ones [5])         ;; vector of 5 ones
(tensor.rand [2 4] 42)   ;; 2x4 random tensor with seed 42
(tensor.identity 3)       ;; 3x3 identity matrix
```

#### Element-wise Operations

| Function | Args | Description |
|----------|------|-------------|
| `tensor.add` | `(tensor.add a b)` | Element-wise addition (same shape) |
| `tensor.mul` | `(tensor.mul a b)` | Element-wise multiplication (same shape) |

#### Matrix Operations

| Function | Args | Description |
|----------|------|-------------|
| `tensor.matmul` | `(tensor.matmul a b)` | Matrix multiplication |
| `tensor.transpose` | `(tensor.transpose t)` | Transpose a 2D tensor |

#### Shape Operations

| Function | Args | Description |
|----------|------|-------------|
| `tensor.reshape` | `(tensor.reshape t [new-shape])` | Reshape (total elements must match) |
| `tensor.slice` | `(tensor.slice t start end)` | Slice first dimension [start, end) |

#### Reductions

| Function | Args | Returns | Description |
|----------|------|---------|-------------|
| `tensor.sum` | `(tensor.sum t)` | Float | Sum all elements |
| `tensor.max` | `(tensor.max t)` | Float | Maximum element |
| `tensor.softmax` | `(tensor.softmax t)` | Tensor | Softmax normalization |

```lisp
;; Tensor pipeline example
(let (a : tensor (tensor.ones [3 3]))
  (let (b : tensor (tensor.identity 3))
    (let (c : tensor (tensor.matmul a b))
      (print "sum =" (tensor.sum c)))))
```

---

## 6a. `extern-c` Declarations (v1.1.0)

AIRL can call C functions directly using the `extern-c` syntax. This is how the stdlib implements functions that need low-level runtime support:

```lisp
(extern-c "c_function_name" [(param1 : Type1) (param2 : Type2) -> ReturnType])
```

The string is the C symbol name. The signature uses standard AIRL type syntax. Once declared, the function is callable like any other AIRL function:

```lisp
;; Declare a C function
(extern-c "airl_read_file" [(path : String) -> String])

;; Call it
(airl_read_file "data.txt")
```

**When to use:** Stdlib modules use `extern-c` to access runtime primitives (file I/O, system calls, etc.) that cannot be implemented in pure AIRL. User code rarely needs `extern-c` directly — prefer stdlib functions.

---

## 7. Contracts

Every function must have at least one contract clause (`:requires` or `:ensures`). Contracts are boolean expressions checked at runtime.

### :requires — Preconditions

Checked **before** the function body runs. If any clause evaluates to non-`true`, a `ContractViolation` error is raised.

```lisp
:requires [(> denominator 0) (valid numerator)]
```

### :ensures — Postconditions

Checked **after** the body runs. The special variable `result` is bound to the return value.

```lisp
:ensures [(>= result 0) (<= result 100)]
```

### :invariant — Invariants

Checked after the body, **before** `:ensures`. Same semantics as `:ensures` but categorized separately for error reporting.

```lisp
:invariant [(>= result 0)]
```

### The `valid` Guard

`(valid x)` always returns `true`. Use it as a minimal contract when you have no meaningful precondition:

```lisp
:requires [(valid x)]
```

### Contract Clauses Are Lists

Each contract clause is a bracket list of boolean expressions. All must evaluate to `true`:

```lisp
:requires [(>= n 0) (<= n 20)]   ;; two separate checks
:ensures [(valid result)]          ;; one check
```

### Z3 Formal Verification

Use `cargo run -- check file.airl` to invoke Z3. The solver attempts to prove contracts statically. It can prove arithmetic properties like `(= result (+ a b))` and `(>= result 0)`.

```lisp
(defn abs
  :sig [(x : i32) -> i32]
  :requires [(valid x)]
  :ensures [(>= result 0)]        ;; Z3 can prove this
  :body (if (< x 0) (* x -1) x))
```

### Contract Gotchas

- **`result` is only available in `:ensures` and `:invariant`**, not in `:requires`.
- **Z3 does not encode the function body** — it proves contracts from constraints alone. Contracts referencing `result` where `result` is not constrained in `:requires` may get "disproven" warnings (these are suppressed automatically).
- **All contract clauses must return Bool.** Returning a non-Bool value is a contract violation.

---

## 8. Ownership and Borrowing

### Annotations

Place ownership annotations before the parameter name in `:sig`:

| Annotation | Syntax | Semantics |
|------------|--------|-----------|
| `own` | `(own x : T)` | Moves the value. Caller cannot use it after the call |
| `ref` | `(ref x : T)` | Immutable borrow. Caller retains the value |
| `mut` | `(mut x : T)` | Mutable borrow |
| `copy` | `(copy x : T)` | Copy semantics (type must be copyable) |
| *(none)* | `(x : T)` | Default — equivalent to `own` |

### Safe Pattern: Borrow then Move

```lisp
(defn consume
  :sig [(own val : i64) -> i64]
  :requires [(valid val)]
  :ensures [(valid result)]
  :body (* val 2))

(defn inspect
  :sig [(ref val : i64) -> i64]
  :requires [(valid val)]
  :ensures [(valid result)]
  :body val)

(let (x : i64 21)
  (do
    (print "inspect:" (inspect x))   ;; borrow — x is still usable
    (print "consume:" (consume x)))) ;; move — x is consumed
```

### Static Linearity Analysis

The compiler detects use-after-move, move-while-borrowed, and branch ownership divergence. These are warnings in `run` mode and errors in `check` mode.

---

## 9. Quantifiers

### Forall

Returns `true` if the body is true for all values in the domain:

```lisp
(forall [i : Nat]
  (where (< i 10))    ;; optional domain filter
  (>= i 0))           ;; body — must return Bool
;; → true
```

### Exists

Returns `true` if the body is true for at least one value:

```lisp
(exists [i : Nat]
  (where (< i 100))
  (= (* i i) 49))     ;; is there a perfect square = 49?
;; → true (i = 7)
```

### Runtime Behavior

At runtime, quantifiers iterate integers 0 through 9,999. The `where` clause filters the domain. `forall` short-circuits on the first `false`; `exists` short-circuits on the first `true`.

### In Contracts

Quantifiers can be used in `:requires` and `:ensures` clauses. Z3 translates them to native SMT quantifiers for formal verification:

```lisp
(defn all-non-negative
  :sig [(n : Nat) -> bool]
  :requires [(valid n)]
  :ensures [(valid result)]
  :body (forall [i : Nat] (where (< i n)) (>= i 0)))
```

---

## 10. Agent Orchestration

AIRL supports multi-agent communication via subprocess spawning and message passing.

### Writing Agent Modules

An agent module is a regular AIRL file that defines functions. When spawned, all its functions become callable by the parent:

```lisp
;; worker.airl — agent module
(defn add
  :sig [(a : i64) (b : i64) -> i64]
  :requires [(valid a) (valid b)]
  :ensures [(= result (+ a b))]
  :body (+ a b))

(defn multiply
  :sig [(a : i64) (b : i64) -> i64]
  :requires [(valid a) (valid b)]
  :ensures [(= result (* a b))]
  :body (* a b))
```

Functions in agent modules have full contracts — the agent guarantees its outputs.

### Spawning Agents

```lisp
(spawn-agent "path/to/worker.airl")  ;; → agent name (String)
```

Spawns a child process running the AIRL binary. The agent initializes, sends a ready signal, and waits for tasks. Returns a string identifier for the agent.

### Synchronous Send

```lisp
(send agent-name "function-name" arg1 arg2 ...)
```

Sends a task to the agent and blocks until the result is returned:

```lisp
(let (w : String (spawn-agent "worker.airl"))
  (let (result : i64 (send w "add" 10 20))
    (print "Result:" result)))  ;; prints 30
```

### Asynchronous Send

```lisp
(send-async agent-name "function-name" arg1 arg2 ...)  ;; → task ID (String)
```

Returns immediately with a task ID. The agent processes the request in the background.

### Await

```lisp
(await task-id)             ;; block until result
(await task-id 5000)        ;; block with 5000ms timeout
```

### Parallel

Await multiple async tasks at once:

```lisp
(let (t1 : String (send-async w "add" 3 4))
  (let (t2 : String (send-async w "multiply" 5 6))
    (let (results : List (parallel [t1 t2]))
      (do
        (print "add:" (at results 0))       ;; 7
        (print "multiply:" (at results 1))))))  ;; 30
```

Optional timeout as second argument: `(parallel [t1 t2] 5000)`.

### Broadcast

Send the same task to multiple agents, return the first successful result:

```lisp
(broadcast [agent1 agent2 agent3] "function-name" arg1 arg2 ...)
```

Runs concurrently — returns as soon as any agent succeeds.

### Retry

Retry a send with exponential backoff:

```lisp
(retry agent-name "function-name" arg1 arg2 ... :max 3)
```

Backoff: 100ms, 200ms, 400ms, etc. Default 3 retries.

### Escalate

Send a structured escalation message to an agent:

```lisp
(escalate agent-name :reason "error message" :data some-value)
```

If the agent has an `__escalate__` function, it is called. Otherwise returns an `(Escalation ...)` variant.

### Any-Agent

Returns the name of the first spawned agent:

```lisp
(any-agent)  ;; → agent name String
```

---

## 10a. Concurrency Model

AIRL supports two levels of concurrency:

**Thread-level (v0.5.0):** Thread-per-task with message-passing channels. `thread-spawn` creates an OS thread running a closure. `channel-new` creates unbounded channels for inter-thread communication. No shared mutable state — channels are the only way threads communicate. See section 6 "Concurrency" for full API.

**Agent-level:** Process isolation + message passing via `spawn-agent`, `send`, `send-async`, `await`, `parallel`, `broadcast`. Agents are separate OS processes communicating via RPC.

Both levels enforce the same principle: **no shared mutable state**. Threads use channels, agents use message passing. No mutexes, atomics, or locks are exposed to AIRL programs.

---

## 10b. Testing

AIRL currently has no test runner, no `deftest` form, and no test discovery.

**Current capabilities:**
- `(assert condition)` — panics if `condition` is `false`
- Contract system (`:requires`, `:ensures`, `:invariant`) — runtime checks or Z3-proven
- Tests are run via external tooling: Rust integration tests invoke `airl run <file>` and check exit codes

**Planned:** `(deftest name body)` form and `airl test <file>` CLI command.

---

## 11. Standard Library (Collections)

AIRL's standard library is written in pure AIRL and auto-loaded as a prelude before user code. No imports needed — all stdlib functions are always available.

As of v1.1.0, the stdlib has **13 modules** across `stdlib/`: `collections.airl`, `math.airl`, `result.airl`, `string.airl`, `map.airl`, `set.airl`, `json.airl`, `base64.airl`, `sha256.airl`, `hmac.airl`, `pbkdf2.airl`, `io.airl`, `path.airl`. The prelude (`stdlib/prelude.airl`) loads all modules automatically.

The collections module provides 18 core list-processing functions:

### Core: map, filter, fold

```lisp
;; map — apply function to each element
(map (fn [x] (* x 2)) [1 2 3 4 5])
;; → [2 4 6 8 10]

;; filter — keep elements matching predicate
(filter (fn [x] (> x 3)) [1 2 3 4 5])
;; → [4 5]

;; fold — left fold with accumulator
(fold (fn [acc x] (+ acc x)) 0 [1 2 3 4 5])
;; → 15
```

### Structural: reverse, concat, zip, flatten

```lisp
(reverse [1 2 3])              ;; → [3 2 1]
(concat [1 2] [3 4 5])         ;; → [1 2 3 4 5]
(zip [1 2 3] [4 5 6])          ;; → [[1 4] [2 5] [3 6]]
(flatten [[1 2] [3] [4 5]])    ;; → [1 2 3 4 5]
```

### Slicing: range, take, drop

```lisp
(range 1 6)                    ;; → [1 2 3 4 5]
(take 3 [10 20 30 40 50])      ;; → [10 20 30]
(drop 2 [10 20 30 40 50])      ;; → [30 40 50]
```

### Searching: any, all, find

```lisp
(any (fn [x] (= (% x 2) 0)) [1 3 4 5])   ;; → true (4 is even)
(all (fn [x] (> x 0)) [1 2 3])             ;; → true
(find (fn [x] (> x 3)) [1 2 3 4 5])        ;; → 4
(find (fn [x] (> x 100)) [1 2 3])          ;; → nil
```

### Sorting: sort, merge

```lisp
;; sort — merge sort with comparison function
(sort (fn [a b] (< a b)) [5 3 1 4 2])     ;; → [1 2 3 4 5]
(sort (fn [a b] (> a b)) [5 3 1 4 2])     ;; → [5 4 3 2 1]

;; merge — merge two sorted lists (used internally by sort)
(merge (fn [a b] (< a b)) [1 3 5] [2 4 6])  ;; → [1 2 3 4 5 6]
```

### Complete Function Reference

| Function | Signature | Returns | Description |
|----------|-----------|---------|-------------|
| `map` | `(map f xs)` | List | Apply f to each element |
| `filter` | `(filter pred xs)` | List | Keep elements where pred returns true |
| `fold` | `(fold f init xs)` | any | Left fold: `f(f(f(init, x1), x2), x3)` |
| `reverse` | `(reverse xs)` | List | Reverse a list |
| `concat` | `(concat xs ys)` | List | Concatenate two lists |
| `zip` | `(zip xs ys)` | List | Pair elements `[[x1 y1] [x2 y2] ...]` |
| `flatten` | `(flatten xss)` | List | Flatten list of lists |
| `range` | `(range start end)` | List | Integers `[start, end)` |
| `take` | `(take n xs)` | List | First n elements |
| `drop` | `(drop n xs)` | List | Skip first n elements |
| `any` | `(any pred xs)` | Bool | Any element satisfies pred? |
| `all` | `(all pred xs)` | Bool | All elements satisfy pred? |
| `find` | `(find pred xs)` | any/nil | First element satisfying pred, or nil |
| `sort` | `(sort cmp xs)` | List | Merge sort with comparison function |
| `merge` | `(merge cmp xs ys)` | List | Merge two sorted lists |

**Note:** All stdlib functions are recursive. For very large lists (>10,000 elements), you may hit the recursion depth limit (50,000). The `zip` function stops at the shorter list.

---

## 12. Standard Library (Math)

Math utility functions, auto-loaded in the prelude. All operate on `i64` integers.

```lisp
(abs -5)               ;; → 5
(min 3 7)              ;; → 3
(max 3 7)              ;; → 7
(clamp 15 0 10)        ;; → 10
(sign -42)             ;; → -1
(even? 4)              ;; → true
(odd? 5)               ;; → true
(pow 2 10)             ;; → 1024
(gcd 12 8)             ;; → 4
(lcm 4 6)              ;; → 12
(sum-list [1 2 3 4 5])     ;; → 15
(product-list [1 2 3 4 5]) ;; → 120
```

| Function | Signature | Description |
|----------|-----------|-------------|
| `abs` | `(abs x)` → i64 | Absolute value |
| `min` | `(min a b)` → i64 | Smaller of two values |
| `max` | `(max a b)` → i64 | Larger of two values |
| `clamp` | `(clamp x lo hi)` → i64 | Clamp to range [lo, hi]. Requires `lo <= hi` |
| `sign` | `(sign x)` → i64 | Returns -1, 0, or 1 |
| `even?` | `(even? x)` → Bool | Is x divisible by 2? |
| `odd?` | `(odd? x)` → Bool | Is x not divisible by 2? |
| `pow` | `(pow base exp)` → i64 | Integer power. Requires `exp >= 0` |
| `gcd` | `(gcd a b)` → i64 | Greatest common divisor (Euclidean). Requires `a >= 0, b >= 0` |
| `lcm` | `(lcm a b)` → i64 | Least common multiple. Requires `a >= 0, b >= 0` |
| `sum-list` | `(sum-list xs)` → i64 | Sum all elements in a list |
| `product-list` | `(product-list xs)` → i64 | Multiply all elements in a list |

---

## 13. Standard Library (Result Combinators)

Combinators for working with `Result` values (`(Ok v)` / `(Err e)`) without verbose `match` expressions. Auto-loaded in the prelude.

```lisp
;; Check variants
(is-ok? (Ok 42))          ;; → true
(is-err? (Err "fail"))    ;; → true

;; Extract with default
(unwrap-or (Ok 42) 0)     ;; → 42
(unwrap-or (Err "fail") 0) ;; → 0

;; Transform Ok or Err values
(map-ok (fn [x] (* x 2)) (Ok 21))     ;; → (Ok 42)
(map-err (fn [e] (+ e "!")) (Err "oops")) ;; → (Err "oops!")

;; Chain operations (monadic bind)
(and-then (fn [x] (if (> x 0) (Ok (* x 2)) (Err "negative"))) (Ok 5))
;; → (Ok 10)

;; Recover from errors
(or-else (fn [e] (Ok 0)) (Err "failed"))  ;; → (Ok 0)

;; Convert nullable to Result
(ok-or 42 "was nil")    ;; → (Ok 42)
(ok-or nil "was nil")   ;; → (Err "was nil")
```

| Function | Signature | Description |
|----------|-----------|-------------|
| `is-ok?` | `(is-ok? r)` → Bool | Check if Result is Ok |
| `is-err?` | `(is-err? r)` → Bool | Check if Result is Err |
| `unwrap-or` | `(unwrap-or r default)` → any | Extract Ok value or return default |
| `map-ok` | `(map-ok f r)` → Result | Apply f to Ok value, pass Err through |
| `map-err` | `(map-err f r)` → Result | Apply f to Err value, pass Ok through |
| `and-then` | `(and-then f r)` → Result | If Ok, apply f (must return Result). If Err, propagate |
| `or-else` | `(or-else f r)` → Result | If Err, apply f to recover. If Ok, pass through |
| `ok-or` | `(ok-or val err)` → Result | Non-nil → `(Ok val)`, nil → `(Err err)` |

---

## 14. Standard Library (String)

String manipulation functions. As of v1.1.0, all string functions (character access, search, transformation, split/join, and helpers) are implemented in pure AIRL in `stdlib/string.airl`. Auto-loaded in the prelude.

### Character Access

```lisp
(char-at "hello" 0)                  ;; → "h"
(chars "abc")                         ;; → ["a" "b" "c"]
(substring "hello world" 0 5)        ;; → "hello"
```

### Search

```lisp
(contains "hello world" "world")     ;; → true
(starts-with "hello" "hel")          ;; → true
(ends-with "hello" "llo")            ;; → true
(index-of "hello world" "world")     ;; → 6
```

### Transformation

```lisp
(to-upper "hello")                    ;; → "HELLO"
(to-lower "HELLO")                    ;; → "hello"
(trim "  hello  ")                    ;; → "hello"
(replace "hello world" "world" "AIRL") ;; → "hello AIRL"
```

### Split, Join, Words, Lines

```lisp
(split "a,b,c" ",")                  ;; → ["a" "b" "c"]
(join ["a" "b" "c"] "-")             ;; → "a-b-c"
(words "  hello   world  ")          ;; → ["hello" "world"]
(unwords ["hello" "world"])          ;; → "hello world"
(lines "line1\nline2")               ;; → ["line1" "line2"]
(unlines ["line1" "line2"])          ;; → "line1\nline2"
```

### Building and Inspection

```lisp
(repeat-str "ab" 3)                  ;; → "ababab"
(pad-left "42" 5 "0")               ;; → "00042"
(pad-right "hi" 5 ".")              ;; → "hi..."
(reverse-str "hello")                ;; → "olleh"
(is-empty-str "")                    ;; → true
(count-occurrences "abcabcabc" "abc") ;; → 3
```

**Note:** All character indexing is Unicode-safe. `length` on strings returns byte length; for character count use `(length (chars s))`. See `stdlib/string.md` for the full reference.

---

## 15. Standard Library (Map / Dictionary)

Hash map with string keys and arbitrary values. O(1) lookups backed by Rust HashMap. Maps are immutable — operations return new maps. Auto-loaded in the prelude.

### Creating and Reading

```lisp
(map-new)                                ;; → {}
(map-from ["name" "AIRL" "version" 1])   ;; → {name: "AIRL", version: 1}
(map-from-entries [["a" 1] ["b" 2]])     ;; → {a: 1, b: 2}

(let (m : _ (map-from ["x" 10 "y" 20]))
  (do
    (map-get m "x")          ;; → 10
    (map-get m "z")          ;; → nil
    (map-get-or m "z" 99)    ;; → 99
    (map-has m "x")          ;; → true
    (map-size m)             ;; → 2
    (map-keys m)             ;; → ["x" "y"]
    (map-values m)           ;; → [10 20]
    (map-entries m)))        ;; → [["x" 10] ["y" 20]]
```

### Modifying and Transforming

```lisp
(map-set m "z" 30)                              ;; add/overwrite key
(map-remove m "x")                               ;; remove key
(map-merge m1 m2)                                ;; merge (m2 wins conflicts)
(map-update m "count" (fn [v] (+ v 1)))          ;; update existing key
(map-map-values (fn [v] (* v 2)) m)              ;; transform all values
(map-filter (fn [k v] (> v 10)) m)               ;; keep matching entries
```

**Note:** Map keys are **string-only**. Passing a non-string key is a runtime error. For integer keys, use `(int-to-string n)` as a workaround. Non-string key support (integer, boolean, composite keys) is a planned future enhancement. This restriction also applies to Sets (implemented as maps with `true` values). See `stdlib/map.md` for the full reference including patterns for symbol tables and frequency counters.

---

## 16. Lambdas and Higher-Order Functions

### Lambda Syntax

```lisp
(fn [x] (+ x 1))           ;; single parameter
(fn [x y] (* x y))         ;; multiple parameters
```

Lambda parameters do **not** need type annotations (unlike `defn` parameters).

### Let-Binding Lambdas

```lisp
(let (square : fn (fn [x] (* x x)))
  (square 7))  ;; → 49
```

### Passing Functions as Arguments

```lisp
(defn apply-twice
  :sig [(f : fn) (x : i64) -> i64]
  :requires [(valid f) (valid x)]
  :ensures [(valid result)]
  :body (f (f x)))

(apply-twice (fn [x] (+ x 1)) 10)  ;; → 12
(apply-twice (fn [x] (* x 2)) 3)   ;; → 12
```

### Closures

Lambdas capture their enclosing scope:

```lisp
(let (offset : i64 10)
  (let (add-offset : fn (fn [x] (+ x offset)))
    (add-offset 5)))  ;; → 15
```

---

## 17. Modules and Type Definitions

### Modules

Modules group definitions with metadata:

```lisp
(module math-utils
  :version 0.1.0
  :verify checked
  :body [
    (defn square
      :sig [(x : i32) -> i32]
      :requires [(valid x)]
      :ensures [(= result (* x x))]
      :body (* x x))
  ])
```

**Note:** Module definitions are parsed and their bodies are evaluated sequentially, but there is no cross-file module resolution at runtime.

### Type Definitions

Define sum types (enums) and product types (structs):

```lisp
;; Sum type
(deftype Shape
  :body (Sum
    (Circle [f64])
    (Rectangle [f64 f64])
    (Triangle [f64 f64 f64])))

;; Product type
(deftype Point
  :body (Product
    (x f64)
    (y f64)))
```

### Use Declarations

```lisp
(use math-utils :all)
(use math-utils :symbols [square cube])
(use math-utils :prefixed mu)
```

**Note:** Parsed but no cross-file resolution at runtime.

---

## 18. Common Patterns

### Safe Division with Result

```lisp
(defn safe-divide
  :sig [(a : i64) (b : i64) -> Result[i64, String]]
  :requires [(valid a) (valid b)]
  :ensures [(valid result)]
  :body (if (= b 0)
          (Err "division by zero")
          (Ok (/ a b))))

(match (safe-divide 10 3)
  (Ok v)  (print "Result:" v)
  (Err e) (print "Error:" e))
```

### Chained Let Bindings

Use multi-binding `let` to chain computations (preferred over nested single-binding lets):

```lisp
(let (a : i64 10)
     (b : i64 (* a 2))
     (c : i64 (+ b 5))
  (print "Result:" c))  ;; prints 25
```

### Error Propagation with Nested Match

```lisp
(match (safe-divide 100 5)
  (Ok first) (match (safe-divide first 2)
    (Ok second) (print "Final:" second)
    (Err e) (print "Error:" e))
  (Err e) (print "Error:" e))
```

### Tensor Pipeline

```lisp
(let (data : tensor (tensor.rand [4 4] 42))
  (let (normalized : tensor (tensor.softmax data))
    (let (transposed : tensor (tensor.transpose normalized))
      (print "Sum:" (tensor.sum transposed)))))
```

### Multi-Agent Delegation

```lisp
(let (w : String (spawn-agent "worker.airl"))
  (do
    ;; Fire off parallel work
    (let (t1 : String (send-async w "task-a" 10))
      (let (t2 : String (send-async w "task-b" 20))
        (let (results : List (parallel [t1 t2]))
          (print "Done:" results))))))
```

### Imperative → Functional Translations

If you are tempted to use a loop, here is how to translate common imperative patterns:

**Power / exponentiation (loop with accumulator → fold):**
```lisp
;; WRONG — AIRL has no while or set!
;; (let (result 1) (while (< i n) (set! result (* result base))))

;; CORRECT — fold over a range
(defn power
  :sig [(base : i64) (exp : i64) -> i64]
  :requires [(>= exp 0)]
  :ensures [(valid result)]
  :body (fold (fn [acc _] (* acc base)) 1 (range 0 exp)))
```

**Build a list from indices (loop appending → map with range):**
```lisp
;; WRONG — no while/set!/append-in-loop
;; (let (acc []) (while (< i n) (set! acc (append acc (f i)))))

;; CORRECT — map over a range
(map (fn [i] (* i i)) (range 0 5))    ;; → [0 1 4 9 16]
```

**Running sum (loop with running total → fold building a list):**
```lisp
(defn running-sum
  :sig [(xs : List) -> List]
  :requires [(valid xs)]
  :ensures [(= (length result) (length xs))]
  :body (if (empty? xs)
          []
          (let (first-val : i64 (head xs))
            (fold (fn [acc x]
                    (append acc (+ (at acc (- (length acc) 1)) x)))
                  [first-val]
                  (tail xs)))))
```

**Counting occurrences (loop with counter → fold):**
```lisp
(defn count-vowels
  :sig [(s : String) -> i64]
  :requires [(valid s)]
  :ensures [(>= result 0)]
  :body (let (chars-list : List (chars s))
          (fold (fn [count c]
                  (if (or (= c "a") (or (= c "e") (or (= c "i") (or (= c "o") (= c "u")))))
                    (+ count 1)
                    count))
                0 chars-list)))
                0 chars-list)))
```

**Zip two lists (indexed loop → fold with range):**
```lisp
(defn zip-lists
  :sig [(xs : List) (ys : List) -> List]
  :requires [(valid xs) (valid ys)]
  :ensures [(valid result)]
  :body (let (len : i64 (min (length xs) (length ys)))
          (map (fn [i] [(at xs i) (at ys i)]) (range 0 len))))
```

**Merge sorted lists (while with two pointers → recursion):**
```lisp
(defn merge-sorted
  :sig [(xs : List) (ys : List) -> List]
  :requires [(valid xs) (valid ys)]
  :ensures [(valid result)]
  :body (if (empty? xs) ys
          (if (empty? ys) xs
            (if (<= (head xs) (head ys))
              (cons (head xs) (merge-sorted (tail xs) ys))
              (cons (head ys) (merge-sorted xs (tail ys)))))))
```

---

## 19. Gotchas and Common Mistakes

1. **Missing else branch.** `(if cond then)` is a parse error. Always provide both branches: `(if cond then else)`.

2. **Type mismatch in arithmetic.** `(+ 1 1.0)` fails — both operands must be the same type. Use `(+ 1.0 1.0)` for floats.

3. **Forgetting contracts.** Every `defn` needs at least one `:requires` or `:ensures`. Use `(valid x)` as a minimal guard.

4. **`result` in `:requires`.** The `result` variable is only bound after body evaluation. It is only available in `:ensures` and `:invariant`, not `:requires`.

5. **Match arm count.** After the scrutinee, items must come in pattern/body pairs. An odd count is a parse error.

6. **Variant constructors must be uppercase.** `(ok 42)` is a function call. `(Ok 42)` is a variant constructor.

7. **Keywords evaluate to strings.** `:foo` evaluates to the string `":foo"`. They are syntax sugar, not a separate type.

8. **Tensor creation takes no element type.** Write `(tensor.zeros [3 3])`, **not** `(tensor.zeros f32 [3 3])`. All tensors are f32 internally.

9. **`tensor.rand` needs a seed.** `(tensor.rand [3 3] 42)` — the second argument is an integer seed.

10. **Lambda params have no types.** Write `(fn [x y] body)`, **not** `(fn [(x : i32) (y : i32)] body)`.

11. **`spawn-agent` takes a file path.** Write `(spawn-agent "worker.airl")`, **not** `(spawn-agent :name "worker" :capabilities [...])`.

12. **`send` is positional.** Write `(send agent "fn-name" arg1 arg2)`, **not** `(send agent (task ...))`.

13. **Division of integers is integer division.** `(/ 10 3)` returns `3`, not `3.333...`. For float division use `(/ 10.0 3.0)`.

14. **Let bindings require type annotations.** Write `(let (x : i64 5) ...)`, **not** `(let (x 5) ...)`.

15. **No loops or mutation.** AIRL has no `while`, `for`, `set!`, or variable reassignment. Use `fold`, `map`, `filter`, or recursion. See the "Imperative → Functional Translations" section above.

16. **`empty?` is for lists only.** To check if a string is empty, use `(= s "")` or `(= (length s) 0)`. `(empty? some-string)` is a runtime error.

17. **`let` must have a body expression.** `(let (x : i64 5))` is invalid — there's no body. Write `(let (x : i64 5) (+ x 1))`. The body is the expression after the binding, before the closing paren.

18. **`if` has exactly 3 subforms.** `(if cond then else)` — condition, then-branch, else-branch. No more, no less. If you need multiple statements in a branch, wrap them in `do`: `(if cond (do a b c) else-expr)`.

19. **Use `char-code`/`char-from-code`, not `ord`/`chr`.** AIRL uses `(char-code s)` to get the Unicode code point of the first character, and `(char-from-code n)` to convert a code point back to a single-character string. Python-style `ord`/`chr` do not exist.
