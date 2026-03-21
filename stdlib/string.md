# AIRL Standard Library: String

> Source: `stdlib/string.airl` + 13 Rust builtins | 23 functions total | Auto-loaded

String manipulation functions. The 13 core operations are Rust builtins (character-level access requires native code). The 10 higher-level helpers are pure AIRL built on top. All functions are available automatically — no imports needed.

## Dependencies

Higher-level functions depend on Collections (`filter`, `reverse`) and string builtins (`chars`, `join`, `split`, `trim`).

## Builtin Functions (Rust)

### Character Access

```lisp
(char-at "hello" 0)              ;; → "h"
(char-at "hello" 4)              ;; → "o"

(chars "abc")                     ;; → ["a" "b" "c"]
(chars "")                        ;; → []

(substring "hello world" 0 5)    ;; → "hello"
(substring "hello world" 6 11)   ;; → "world"
```

### Search

```lisp
(contains "hello world" "world")    ;; → true
(contains "hello world" "xyz")      ;; → false

(starts-with "hello" "hel")         ;; → true
(ends-with "hello" "llo")           ;; → true

(index-of "hello world" "world")    ;; → 6  (char index)
(index-of "hello" "xyz")            ;; → -1 (not found)
```

### Transformation

```lisp
(to-upper "hello")                   ;; → "HELLO"
(to-lower "HELLO")                   ;; → "hello"
(trim "  hello  ")                   ;; → "hello"
(replace "hello world" "world" "AIRL")  ;; → "hello AIRL"
```

### Split and Join

```lisp
(split "a,b,c" ",")                 ;; → ["a" "b" "c"]
(split "hello" "")                  ;; → ["h" "e" "l" "l" "o"]
(join ["a" "b" "c"] "-")            ;; → "a-b-c"
(join ["hello" "world"] " ")        ;; → "hello world"
```

## AIRL Helper Functions

### Word and Line Processing

```lisp
(words "  hello   world  ")         ;; → ["hello" "world"]
(unwords ["hello" "world"])         ;; → "hello world"

(lines "line1\nline2\nline3")       ;; → ["line1" "line2" "line3"]
(unlines ["line1" "line2"])         ;; → "line1\nline2"
```

### Building and Padding

```lisp
(repeat-str "ab" 3)                 ;; → "ababab"
(repeat-str "x" 0)                  ;; → ""

(pad-left "42" 5 "0")              ;; → "00042"
(pad-right "hi" 5 ".")             ;; → "hi..."
```

### Inspection

```lisp
(is-empty-str "")                   ;; → true
(is-empty-str "hi")                 ;; → false

(reverse-str "hello")               ;; → "olleh"

(count-occurrences "abcabcabc" "abc")  ;; → 3
(count-occurrences "hello" "l")        ;; → 2
```

## Builtin Function Reference

| Function | Signature | Returns | Description |
|----------|-----------|---------|-------------|
| `char-at` | `(char-at s i)` | String | Character at index i (Unicode-safe). Errors on out of bounds |
| `substring` | `(substring s start end)` | String | Extract chars [start, end). Unicode-safe |
| `split` | `(split s delim)` | List | Split by delimiter string |
| `join` | `(join list sep)` | String | Join list of strings with separator |
| `contains` | `(contains s sub)` | Bool | Does s contain sub? |
| `starts-with` | `(starts-with s prefix)` | Bool | Does s start with prefix? |
| `ends-with` | `(ends-with s suffix)` | Bool | Does s end with suffix? |
| `trim` | `(trim s)` | String | Remove leading/trailing whitespace |
| `to-upper` | `(to-upper s)` | String | Convert to uppercase |
| `to-lower` | `(to-lower s)` | String | Convert to lowercase |
| `replace` | `(replace s old new)` | String | Replace all occurrences of old with new |
| `index-of` | `(index-of s sub)` | Int | Character index of first occurrence, or -1 |
| `chars` | `(chars s)` | List | Convert to list of single-char strings |

## AIRL Helper Function Reference

| Function | Signature | Returns | Description |
|----------|-----------|---------|-------------|
| `words` | `(words s)` | List | Split by whitespace, ignoring empty segments |
| `unwords` | `(unwords ws)` | String | Join with spaces |
| `lines` | `(lines s)` | List | Split by newlines |
| `unlines` | `(unlines ls)` | String | Join with newlines |
| `repeat-str` | `(repeat-str s n)` | String | Repeat string n times. Requires `n >= 0` |
| `pad-left` | `(pad-left s width ch)` | String | Pad to width by prepending ch |
| `pad-right` | `(pad-right s width ch)` | String | Pad to width by appending ch |
| `is-empty-str` | `(is-empty-str s)` | Bool | Is string empty? |
| `reverse-str` | `(reverse-str s)` | String | Reverse a string. Ensures length preserved |
| `count-occurrences` | `(count-occurrences s sub)` | Int | Count non-overlapping occurrences. Requires non-empty sub |

## Notes

- All character indexing is **Unicode-safe** (uses Rust's `.chars()` iterator, not byte indices)
- `length` on strings returns **byte length**, not character count. For char count, use `(length (chars s))`
- `index-of` returns the **character index**, not byte offset
- `words` uses `split` + `filter` to handle multiple spaces; `split` alone would produce empty strings
- `pad-left`/`pad-right` use `length` (byte-based) for width comparison — correct for ASCII, may over-pad for multi-byte Unicode
- For self-hosting: `char-at`, `substring`, `chars`, and `split` provide the character-level access needed to implement a tokenizer/parser in AIRL
