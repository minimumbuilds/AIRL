You are generating code in AIRL (AI Intermediate Representation Language), an S-expression language with mandatory contracts. Here is a condensed reference.

## Syntax

Everything is an S-expression: `(operator arg1 arg2 ...)`. Square brackets `[...]` are used for lists, parameter lists, and contract clauses.

### Literals
- Integer: `42`, `-7`, `0` (i64)
- Float: `3.14`, `-0.5` (f64) -- do NOT mix with integers in arithmetic
- String: `"hello"`
- Boolean: `true`, `false`
- Nil: `nil`
- List: `[1 2 3]`

### Let Binding (type annotation REQUIRED)
```
(let (x : i64 5) (+ x 1))
(let (x : i64 5) (y : i64 10) (+ x y))
```

### If Expression (BOTH branches required)
```
(if (< x 0) (* x -1) x)
```

### Do Block (sequence, returns last)
```
(do (print "step 1") (print "step 2") 42)
```

### Match Expression (pattern/body pairs, no extra parens)
```
(match expr
  (Ok v)  (print "ok:" v)
  (Err e) (print "err:" e))
```

Patterns: binding `x`, wildcard `_`, literal `42`, variant `(Ok v)`, nested `(Ok (Some v))`.

### Lambda
```
(fn [x] (+ x 1))
(fn [x y] (* x y))
```
Lambda params have NO type annotations.

## Function Definition

```
(defn function-name
  :sig [(param1 : Type1) (param2 : Type2) -> ReturnType]
  :requires [precondition1 precondition2]
  :ensures [postcondition1 postcondition2]
  :body expression)
```

- `:sig` and `:body` are REQUIRED
- At least one of `:requires` or `:ensures` is REQUIRED
- In `:ensures`, the special variable `result` holds the return value
- `(valid x)` always returns true -- use as minimal contract guard

## Types
- Integers: `i8`, `i16`, `i32`, `i64` (all stored as i64 at runtime)
- Unsigned: `u8`, `u16`, `u32`, `u64`
- Floats: `f16`, `f32`, `f64`, `bf16`
- `bool`, `String`, `List`, `fn`
- `Result[T, E]` with variants `(Ok value)` and `(Err reason)`

## Builtin Functions

### Arithmetic (2 args, same type, no coercion)
`+`, `-`, `*`, `/` (integer division for ints), `%`

### Comparison (2 args, returns bool)
`=`, `!=`, `<`, `>`, `<=`, `>=`

### Logic
`and`, `or` (2 args), `not` (1 arg)

### Collections
- `(length list-or-string)` -- returns Int
- `(at list index)` -- 0-based indexing
- `(append list element)` -- returns new list
- `(filter (fn [x] condition) list)` -- keep elements where condition is true
- `(map (fn [x] expr) list)` -- transform each element
- `(fold (fn [acc x] expr) initial list)` -- reduce left
- `(empty? list)` -- returns bool
- `(head list)` -- first element
- `(tail list)` -- all but first
- `(cons element list)` -- prepend element

### String
- `(split string delimiter)` -- split string into list
- `(trim string)` -- remove leading/trailing whitespace

### Utility
- `(print arg1 arg2 ...)` -- print to stdout
- `(valid x)` -- always returns true
- `(type-of x)` -- returns type name as String

## Key Gotchas
1. Both `if` branches are REQUIRED -- `(if cond then)` is a parse error
2. `let` REQUIRES type annotations -- `(let (x 5) ...)` is wrong, use `(let (x : i64 5) ...)`
3. Every `defn` MUST have at least one `:requires` or `:ensures`
4. `result` is ONLY available in `:ensures`, NOT in `:requires`
5. Variant constructors are UPPERCASE: `(Ok 42)`, `(Err "msg")`, NOT `(ok 42)`
6. Integer division: `(/ 10 3)` returns `3`, not `3.333`
7. No type coercion: `(+ 1 1.0)` is an error

Generate a complete, runnable AIRL program for the task below. Output ONLY the AIRL code -- no explanations, no markdown fences. The program must:
1. Define the function with proper :sig, :requires, :ensures, and :body
2. Call the function with the specified test input
3. Print the result

TASK:
