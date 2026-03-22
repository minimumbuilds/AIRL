# Self-Hosted Lexer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a self-hosted AIRL lexer in pure AIRL that tokenizes source strings into a list of typed tokens.

**Architecture:** A single AIRL source file (`bootstrap/lexer.airl`) containing ~15 functions that use index-walking recursion to tokenize input. State is passed as `(source pos line col)` through recursive calls. All functions return `(Ok ...)` or `(Err msg)` for error propagation.

**Tech Stack:** Pure AIRL — uses only existing builtins (`char-at`, `substring`, `length`, `contains`, `index-of`) and stdlib (`reverse`). No Rust changes required.

**Spec:** `docs/superpowers/specs/2026-03-22-self-hosted-lexer-design.md`

---

## File Structure

| File | Purpose |
|------|---------|
| Create: `bootstrap/lexer.airl` | The self-hosted lexer — all functions |
| Create: `bootstrap/lexer_test.airl` | AIRL test program that validates the lexer |
| Create: `tests/fixtures/valid/lexer_bootstrap.airl` | Rust fixture test — run lexer on small input |

No Rust files modified. No stdlib changes. The lexer is a pure user-space AIRL program.

---

### Task 1: Helper Predicates and Assert

**Files:**
- Create: `bootstrap/lexer.airl`
- Create: `bootstrap/lexer_test.airl`

Write the foundation: character classification predicates and the test helper.

- [ ] **Step 1: Create `bootstrap/` directory and `lexer.airl` with helper predicates**

```clojure
;; bootstrap/lexer.airl — Self-hosted AIRL lexer

;; ── Helper predicates ───────────────────────────────

(defn is-whitespace?
  :sig [(ch : String) -> Bool]
  :intent "Check if single-char string is whitespace"
  :requires [(valid ch)]
  :ensures [(valid result)]
  :body (or (= ch " ") (or (= ch "\t") (or (= ch "\n") (= ch "\r")))))

(defn is-digit?
  :sig [(ch : String) -> Bool]
  :intent "Check if single-char string is a decimal digit"
  :requires [(valid ch)]
  :ensures [(valid result)]
  :body (contains "0123456789" ch))

(defn digit-value
  :sig [(ch : String) -> i64]
  :intent "Return numeric value of a digit character"
  :requires [(valid ch)]
  :ensures [(valid result)]
  :body (index-of "0123456789" ch))

(defn is-symbol-start?
  :sig [(ch : String) -> Bool]
  :intent "Check if char can start a symbol"
  :requires [(valid ch)]
  :ensures [(valid result)]
  :body (contains "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!$%&*+-./<=>?@^_~" ch))

(defn is-symbol-char?
  :sig [(ch : String) -> Bool]
  :intent "Check if char can continue a symbol"
  :requires [(valid ch)]
  :ensures [(valid result)]
  :body (or (is-symbol-start? ch) (is-digit? ch)))
```

- [ ] **Step 2: Create `bootstrap/lexer_test.airl` with assert-eq and first tests**

```clojure
;; bootstrap/lexer_test.airl — Tests for the self-hosted lexer

;; ── Test helper ─────────────────────────────────────

(defn assert-eq
  :sig [(a : Any) (b : Any) -> Bool]
  :intent "Assert equality, print error if not"
  :requires [(valid a) (valid b)]
  :ensures [(valid result)]
  :body (if (= a b) true
          (do (print "FAIL: expected" b "got" a) false)))

;; ── Helper predicate tests ──────────────────────────

(do
  (assert-eq (is-whitespace? " ") true)
  (assert-eq (is-whitespace? "\n") true)
  (assert-eq (is-whitespace? "a") false)
  (assert-eq (is-digit? "5") true)
  (assert-eq (is-digit? "a") false)
  (assert-eq (digit-value "0") 0)
  (assert-eq (digit-value "9") 9)
  (assert-eq (is-symbol-start? "a") true)
  (assert-eq (is-symbol-start? "+") true)
  (assert-eq (is-symbol-start? "5") false)
  (assert-eq (is-symbol-char? "a") true)
  (assert-eq (is-symbol-char? "5") true)
  (print "helper predicate tests passed"))
```

- [ ] **Step 3: Run the test to verify helpers work**

Run: `cargo run -- run bootstrap/lexer_test.airl`
Expected: prints "helper predicate tests passed"

- [ ] **Step 4: Commit**

```bash
git add bootstrap/lexer.airl bootstrap/lexer_test.airl
git commit -m "feat(bootstrap): add lexer helper predicates and test scaffold"
```

---

### Task 2: Whitespace and Comment Skipping

**Files:**
- Modify: `bootstrap/lexer.airl`
- Modify: `bootstrap/lexer_test.airl`

Implement `skip-ws` — skip spaces, tabs, newlines, line comments (`;`), and nestable block comments (`#|...|#`). Track line/col.

- [ ] **Step 1: Add `skip-ws` and `skip-block-comment` to `lexer.airl`**

```clojure
;; ── Whitespace and comment skipping ─────────────────

(defn skip-block-comment
  :sig [(source : String) (pos : i64) (line : i64) (col : i64) (depth : i64) -> List]
  :intent "Skip nestable block comment, return [pos line col] or (Err msg)"
  :requires [(valid source)]
  :ensures [(valid result)]
  :body
    (if (>= pos (length source))
      (Err (+ "unterminated block comment at line:" (+ "" line)))
      (let (ch : String (char-at source pos))
        (if (and (= ch "#") (and (< (+ pos 1) (length source)) (= (char-at source (+ pos 1)) "|")))
          ;; Nested open
          (skip-block-comment source (+ pos 2) line (+ col 2) (+ depth 1))
          (if (and (= ch "|") (and (< (+ pos 1) (length source)) (= (char-at source (+ pos 1)) "#")))
            ;; Close
            (if (= depth 1)
              (Ok [(+ pos 2) line (+ col 2)])
              (skip-block-comment source (+ pos 2) line (+ col 2) (- depth 1)))
            ;; Regular char inside comment
            (if (= ch "\n")
              (skip-block-comment source (+ pos 1) (+ line 1) 0 depth)
              (skip-block-comment source (+ pos 1) line (+ col 1) depth)))))))

(defn skip-ws
  :sig [(source : String) (pos : i64) (line : i64) (col : i64) -> List]
  :intent "Skip whitespace and comments, return (Ok [pos line col]) or (Err msg)"
  :requires [(valid source)]
  :ensures [(valid result)]
  :body
    (if (>= pos (length source))
      (Ok [pos line col])
      (let (ch : String (char-at source pos))
        (if (= ch "\n")
          (skip-ws source (+ pos 1) (+ line 1) 0)
          (if (or (= ch " ") (or (= ch "\t") (= ch "\r")))
            (skip-ws source (+ pos 1) line (+ col 1))
            (if (= ch ";")
              ;; Line comment — skip to end of line
              (skip-line-comment source (+ pos 1) line col)
              (if (and (= ch "#") (and (< (+ pos 1) (length source)) (= (char-at source (+ pos 1)) "|")))
                ;; Block comment
                (match (skip-block-comment source (+ pos 2) line (+ col 2) 1)
                  (Ok state) (skip-ws source (head state) (at state 1) (at state 2))
                  (Err msg) (Err msg))
                ;; Not whitespace
                (Ok [pos line col]))))))))

(defn skip-line-comment
  :sig [(source : String) (pos : i64) (line : i64) (col : i64) -> List]
  :intent "Skip to end of line, then continue skipping whitespace"
  :requires [(valid source)]
  :ensures [(valid result)]
  :body
    (if (>= pos (length source))
      (Ok [pos line col])
      (if (= (char-at source pos) "\n")
        (skip-ws source (+ pos 1) (+ line 1) 0)
        (skip-line-comment source (+ pos 1) line (+ col 1)))))
```

- [ ] **Step 2: Add whitespace tests to `lexer_test.airl`**

```clojure
;; ── Whitespace tests ────────────────────────────────

(do
  ;; Simple whitespace
  (assert-eq (skip-ws "  hello" 0 1 0) (Ok [2 1 2]))
  ;; Newline tracking
  (assert-eq (skip-ws "\n  x" 0 1 0) (Ok [3 2 2]))
  ;; Line comment
  (assert-eq (skip-ws "; comment\nx" 0 1 0) (Ok [10 2 0]))
  ;; Block comment
  (assert-eq (skip-ws "#| block |#x" 0 1 0) (Ok [11 1 11]))
  ;; Nested block comment
  (assert-eq (skip-ws "#| outer #| inner |# still |#x" 0 1 0) (Ok [30 1 30]))
  ;; Unterminated block comment
  (match (skip-ws "#| oops" 0 1 0)
    (Ok _) (print "FAIL: should have errored on unterminated block comment")
    (Err msg) (assert-eq (contains msg "unterminated") true))
  ;; No whitespace
  (assert-eq (skip-ws "hello" 0 1 0) (Ok [0 1 0]))
  ;; End of input
  (assert-eq (skip-ws "" 0 1 0) (Ok [0 1 0]))
  (print "whitespace tests passed"))
```

- [ ] **Step 3: Run tests**

Run: `cargo run -- run bootstrap/lexer_test.airl`
Expected: prints "helper predicate tests passed" and "whitespace tests passed"

- [ ] **Step 4: Commit**

```bash
git add bootstrap/lexer.airl bootstrap/lexer_test.airl
git commit -m "feat(bootstrap): add whitespace and comment skipping"
```

---

### Task 3: Single-Character Tokens and `next-token` Dispatch

**Files:**
- Modify: `bootstrap/lexer.airl`
- Modify: `bootstrap/lexer_test.airl`

Implement `next-token` with dispatch for single-char tokens (`( ) [ ] , :`), arrow (`->`), and stub fallthrough. Implement `lex` and `lex-loop`.

- [ ] **Step 1: Add `next-token`, `lex-loop`, and `lex` to `lexer.airl`**

```clojure
;; ── Core lexer ──────────────────────────────────────

(defn next-token
  :sig [(source : String) (pos : i64) (line : i64) (col : i64) -> List]
  :intent "Read next token from source at pos, return (Ok [token pos line col]) or (Err msg)"
  :requires [(valid source)]
  :ensures [(valid result)]
  :body
    (match (skip-ws source pos line col)
      (Err msg) (Err msg)
      (Ok ws-state)
        (let (p : i64 (head ws-state))
          (let (ln : i64 (at ws-state 1))
            (let (c : i64 (at ws-state 2))
              (if (>= p (length source))
                (Ok [(Token "eof" nil ln c) p ln c])
                (let (ch : String (char-at source p))
                  (if (= ch "(") (Ok [(Token "lparen" "(" ln c) (+ p 1) ln (+ c 1)])
                  (if (= ch ")") (Ok [(Token "rparen" ")" ln c) (+ p 1) ln (+ c 1)])
                  (if (= ch "[") (Ok [(Token "lbracket" "[" ln c) (+ p 1) ln (+ c 1)])
                  (if (= ch "]") (Ok [(Token "rbracket" "]" ln c) (+ p 1) ln (+ c 1)])
                  (if (= ch ",") (Ok [(Token "comma" "," ln c) (+ p 1) ln (+ c 1)])
                  (if (= ch "\"") (lex-string source (+ p 1) ln (+ c 1) "" ln c)
                  (if (= ch ":")
                    (if (and (< (+ p 1) (length source)) (is-symbol-start? (char-at source (+ p 1))))
                      (lex-keyword source (+ p 1) ln (+ c 1) "" ln c)
                      (Ok [(Token "colon" ":" ln c) (+ p 1) ln (+ c 1)]))
                  (if (= ch "-")
                    (if (and (< (+ p 1) (length source)) (= (char-at source (+ p 1)) ">"))
                      (Ok [(Token "arrow" "->" ln c) (+ p 2) ln (+ c 2)])
                      (if (and (< (+ p 1) (length source)) (is-digit? (char-at source (+ p 1))))
                        (lex-number source (+ p 1) ln (+ c 1) 0 true ln c)
                        (lex-symbol source p ln c "" ln c)))
                  (if (is-digit? ch)
                    (lex-number source p ln c 0 false ln c)
                  (if (is-symbol-start? ch)
                    (lex-symbol source p ln c "" ln c)
                    (Err (+ "unexpected character '" (+ ch (+ "' at line:" (+ "" ln)))))
                  ))))))))))))))))))

(defn lex-loop
  :sig [(source : String) (pos : i64) (line : i64) (col : i64) (tokens : List) -> List]
  :intent "Accumulate tokens until EOF, return (Ok tokens) or (Err msg)"
  :requires [(valid source)]
  :ensures [(valid result)]
  :body
    (match (next-token source pos line col)
      (Err msg) (Err msg)
      (Ok result)
        (let (tok : Any (head result))
          (let (new-pos : i64 (at result 1))
            (let (new-line : i64 (at result 2))
              (let (new-col : i64 (at result 3))
                (let (new-tokens : List (cons tok tokens))
                  ;; Check if we hit EOF
                  (match tok
                    (Token kind _ _ _)
                      (if (= kind "eof")
                        (Ok new-tokens)
                        (lex-loop source new-pos new-line new-col new-tokens))))))))))

(defn lex
  :sig [(source : String) -> List]
  :intent "Tokenize source string, return (Ok token-list) or (Err msg)"
  :requires [(valid source)]
  :ensures [(valid result)]
  :body
    (match (lex-loop source 0 1 0 [])
      (Ok tokens) (Ok (reverse tokens))
      (Err msg) (Err msg)))
```

- [ ] **Step 2: Add single-char and dispatch tests to `lexer_test.airl`**

```clojure
;; ── Single token tests ──────────────────────────────

(do
  ;; Delimiters
  (match (lex "()")
    (Ok tokens) (do
      (assert-eq (length tokens) 3)
      (assert-eq (at tokens 0) (Token "lparen" "(" 1 0))
      (assert-eq (at tokens 1) (Token "rparen" ")" 1 1))
      (assert-eq (at tokens 2) (Token "eof" nil 1 2)))
    (Err msg) (print "FAIL:" msg))

  ;; Brackets and comma
  (match (lex "[a, b]")
    (Ok tokens) (assert-eq (length tokens) 6)
    (Err msg) (print "FAIL:" msg))

  ;; Arrow
  (match (lex "->")
    (Ok tokens) (do
      (assert-eq (at tokens 0) (Token "arrow" "->" 1 0))
      (assert-eq (length tokens) 2))
    (Err msg) (print "FAIL:" msg))

  ;; Colon (bare)
  (match (lex ":")
    (Ok tokens) (do
      (assert-eq (at tokens 0) (Token "colon" ":" 1 0)))
    (Err msg) (print "FAIL:" msg))

  ;; Empty input
  (match (lex "")
    (Ok tokens) (do
      (assert-eq (length tokens) 1)
      (assert-eq (at tokens 0) (Token "eof" nil 1 0)))
    (Err msg) (print "FAIL:" msg))

  (print "single token tests passed"))
```

- [ ] **Step 3: Run tests (sub-lexers not yet implemented — test only single-char tokens)**

Run: `cargo run -- run bootstrap/lexer_test.airl`
Expected: prints "single token tests passed"

Note: Tests for symbols/keywords/numbers/strings will be added in subsequent tasks after their sub-lexers are implemented.

- [ ] **Step 4: Commit**

```bash
git add bootstrap/lexer.airl bootstrap/lexer_test.airl
git commit -m "feat(bootstrap): add next-token dispatch, lex-loop, lex entry point"
```

---

### Task 4: Symbol and Keyword Sub-Lexers

**Files:**
- Modify: `bootstrap/lexer.airl`
- Modify: `bootstrap/lexer_test.airl`

Implement `lex-symbol` (reads symbol chars, checks for `true`/`false`/`nil`) and `lex-keyword` (reads symbol chars after `:`).

- [ ] **Step 1: Add `lex-symbol` and `lex-keyword` to `lexer.airl`**

```clojure
;; ── Sub-lexers ──────────────────────────────────────

(defn read-symbol-chars
  :sig [(source : String) (pos : i64) (line : i64) (col : i64) (acc : String) -> List]
  :intent "Read consecutive symbol characters, return [text pos line col]"
  :requires [(valid source)]
  :ensures [(valid result)]
  :body
    (if (>= pos (length source))
      [acc pos line col]
      (let (ch : String (char-at source pos))
        (if (is-symbol-char? ch)
          (read-symbol-chars source (+ pos 1) line (+ col 1) (+ acc ch))
          [acc pos line col]))))

(defn lex-symbol
  :sig [(source : String) (pos : i64) (line : i64) (col : i64) (acc : String) (start-line : i64) (start-col : i64) -> List]
  :intent "Lex a symbol, checking for true/false/nil"
  :requires [(valid source)]
  :ensures [(valid result)]
  :body
    (let (result : List (read-symbol-chars source pos line col acc))
      (let (text : String (head result))
        (let (new-pos : i64 (at result 1))
          (let (new-line : i64 (at result 2))
            (let (new-col : i64 (at result 3))
              (if (= text "true")  (Ok [(Token "bool" true start-line start-col) new-pos new-line new-col])
              (if (= text "false") (Ok [(Token "bool" false start-line start-col) new-pos new-line new-col])
              (if (= text "nil")   (Ok [(Token "nil" nil start-line start-col) new-pos new-line new-col])
                (Ok [(Token "symbol" text start-line start-col) new-pos new-line new-col]))))))))))

(defn lex-keyword
  :sig [(source : String) (pos : i64) (line : i64) (col : i64) (acc : String) (start-line : i64) (start-col : i64) -> List]
  :intent "Lex a keyword (colon already consumed)"
  :requires [(valid source)]
  :ensures [(valid result)]
  :body
    (let (result : List (read-symbol-chars source pos line col acc))
      (let (text : String (head result))
        (let (new-pos : i64 (at result 1))
          (let (new-line : i64 (at result 2))
            (let (new-col : i64 (at result 3))
              (Ok [(Token "keyword" text start-line start-col) new-pos new-line new-col])))))))
```

- [ ] **Step 2: Add symbol and keyword tests**

```clojure
;; ── Symbol and keyword tests ────────────────────────

(do
  ;; Symbol
  (match (lex "defn")
    (Ok tokens) (assert-eq (at tokens 0) (Token "symbol" "defn" 1 0))
    (Err msg) (print "FAIL:" msg))

  ;; Operator symbols
  (match (lex "+ - * /")
    (Ok tokens) (do
      (assert-eq (at tokens 0) (Token "symbol" "+" 1 0))
      (assert-eq (at tokens 1) (Token "symbol" "-" 1 2))
      (assert-eq (at tokens 2) (Token "symbol" "*" 1 4))
      (assert-eq (at tokens 3) (Token "symbol" "/" 1 6)))
    (Err msg) (print "FAIL:" msg))

  ;; Keyword
  (match (lex ":sig")
    (Ok tokens) (assert-eq (at tokens 0) (Token "keyword" "sig" 1 0))
    (Err msg) (print "FAIL:" msg))

  ;; Booleans and nil
  (match (lex "true false nil")
    (Ok tokens) (do
      (assert-eq (at tokens 0) (Token "bool" true 1 0))
      (assert-eq (at tokens 1) (Token "bool" false 1 5))
      (assert-eq (at tokens 2) (Token "nil" nil 1 11)))
    (Err msg) (print "FAIL:" msg))

  ;; Hyphenated symbol
  (match (lex "my-func")
    (Ok tokens) (assert-eq (at tokens 0) (Token "symbol" "my-func" 1 0))
    (Err msg) (print "FAIL:" msg))

  (print "symbol and keyword tests passed"))
```

- [ ] **Step 3: Run tests**

Run: `cargo run -- run bootstrap/lexer_test.airl`
Expected: prints "symbol and keyword tests passed"

- [ ] **Step 4: Commit**

```bash
git add bootstrap/lexer.airl bootstrap/lexer_test.airl
git commit -m "feat(bootstrap): add symbol and keyword sub-lexers"
```

---

### Task 5: Number Sub-Lexer

**Files:**
- Modify: `bootstrap/lexer.airl`
- Modify: `bootstrap/lexer_test.airl`

Implement `lex-number` — parse decimal integers and floats from character sequences.

- [ ] **Step 1: Add `lex-number` and helpers to `lexer.airl`**

```clojure
(defn read-digits
  :sig [(source : String) (pos : i64) (line : i64) (col : i64) (acc : i64) -> List]
  :intent "Read consecutive digits, accumulate integer value, return [value pos line col]"
  :requires [(valid source)]
  :ensures [(valid result)]
  :body
    (if (>= pos (length source))
      [acc pos line col]
      (let (ch : String (char-at source pos))
        (if (is-digit? ch)
          (read-digits source (+ pos 1) line (+ col 1) (+ (* acc 10) (digit-value ch)))
          [acc pos line col]))))

(defn read-frac-digits
  :sig [(source : String) (pos : i64) (line : i64) (col : i64) (acc : f64) (divisor : f64) -> List]
  :intent "Read fractional digits after '.', return [frac-value pos line col]"
  :requires [(valid source)]
  :ensures [(valid result)]
  :body
    (if (>= pos (length source))
      [acc pos line col]
      (let (ch : String (char-at source pos))
        (if (is-digit? ch)
          (read-frac-digits source (+ pos 1) line (+ col 1)
            (+ acc (/ (+ 0.0 (digit-value ch)) divisor))
            (* divisor 10.0))
          [acc pos line col]))))

(defn lex-number
  :sig [(source : String) (pos : i64) (line : i64) (col : i64) (acc : i64) (negative : Bool) (start-line : i64) (start-col : i64) -> List]
  :intent "Lex an integer or float literal"
  :requires [(valid source)]
  :ensures [(valid result)]
  :body
    (let (int-result : List (read-digits source pos line col acc))
      (let (int-val : i64 (head int-result))
        (let (new-pos : i64 (at int-result 1))
          (let (new-line : i64 (at int-result 2))
            (let (new-col : i64 (at int-result 3))
              ;; Check for '.' followed by digit → float
              (if (and (< new-pos (length source))
                       (and (= (char-at source new-pos) ".")
                            (and (< (+ new-pos 1) (length source))
                                 (is-digit? (char-at source (+ new-pos 1))))))
                ;; Float
                (let (frac-result : List (read-frac-digits source (+ new-pos 1) new-line (+ new-col 1) 0.0 10.0))
                  (let (frac-val : f64 (head frac-result))
                    (let (fp : i64 (at frac-result 1))
                      (let (fl : i64 (at frac-result 2))
                        (let (fc : i64 (at frac-result 3))
                          (let (float-val : f64 (+ (+ 0.0 int-val) frac-val))
                            (Ok [(Token "float" (if negative (- 0.0 float-val) float-val) start-line start-col) fp fl fc])))))))
                ;; Integer
                (Ok [(Token "integer" (if negative (- 0 int-val) int-val) start-line start-col) new-pos new-line new-col]))))))))
```

- [ ] **Step 2: Add number tests**

```clojure
;; ── Number tests ────────────────────────────────────

(do
  ;; Integer
  (match (lex "42")
    (Ok tokens) (assert-eq (at tokens 0) (Token "integer" 42 1 0))
    (Err msg) (print "FAIL:" msg))

  ;; Zero
  (match (lex "0")
    (Ok tokens) (assert-eq (at tokens 0) (Token "integer" 0 1 0))
    (Err msg) (print "FAIL:" msg))

  ;; Negative integer
  (match (lex "-7")
    (Ok tokens) (assert-eq (at tokens 0) (Token "integer" -7 1 0))
    (Err msg) (print "FAIL:" msg))

  ;; Float
  (match (lex "3.14")
    (Ok tokens) (assert-eq (at tokens 0) (Token "float" 3.14 1 0))
    (Err msg) (print "FAIL:" msg))

  ;; Negative float
  (match (lex "-0.5")
    (Ok tokens) (assert-eq (at tokens 0) (Token "float" -0.5 1 0))
    (Err msg) (print "FAIL:" msg))

  ;; Integer followed by symbol (no dot)
  (match (lex "42 x")
    (Ok tokens) (do
      (assert-eq (at tokens 0) (Token "integer" 42 1 0))
      (assert-eq (at tokens 1) (Token "symbol" "x" 1 3)))
    (Err msg) (print "FAIL:" msg))

  (print "number tests passed"))
```

- [ ] **Step 3: Run tests**

Run: `cargo run -- run bootstrap/lexer_test.airl`
Expected: prints "number tests passed"

- [ ] **Step 4: Commit**

```bash
git add bootstrap/lexer.airl bootstrap/lexer_test.airl
git commit -m "feat(bootstrap): add number sub-lexer (integers and floats)"
```

---

### Task 6: String Sub-Lexer

**Files:**
- Modify: `bootstrap/lexer.airl`
- Modify: `bootstrap/lexer_test.airl`

Implement `lex-string` — read characters until closing `"`, handle escape sequences.

- [ ] **Step 1: Add `lex-string` to `lexer.airl`**

```clojure
(defn lex-string
  :sig [(source : String) (pos : i64) (line : i64) (col : i64) (acc : String) (start-line : i64) (start-col : i64) -> List]
  :intent "Lex a string literal (opening quote already consumed)"
  :requires [(valid source)]
  :ensures [(valid result)]
  :body
    (if (>= pos (length source))
      (Err (+ "unterminated string at line:" (+ "" start-line)))
      (let (ch : String (char-at source pos))
        (if (= ch "\"")
          ;; Closing quote
          (Ok [(Token "string" acc start-line start-col) (+ pos 1) line (+ col 1)])
          (if (= ch "\\")
            ;; Escape sequence
            (if (>= (+ pos 1) (length source))
              (Err (+ "unterminated escape at line:" (+ "" line)))
              (let (esc : String (char-at source (+ pos 1)))
                (if (= esc "n")  (lex-string source (+ pos 2) line (+ col 2) (+ acc "\n") start-line start-col)
                (if (= esc "t")  (lex-string source (+ pos 2) line (+ col 2) (+ acc "\t") start-line start-col)
                (if (= esc "\\") (lex-string source (+ pos 2) line (+ col 2) (+ acc "\\") start-line start-col)
                (if (= esc "\"") (lex-string source (+ pos 2) line (+ col 2) (+ acc "\"") start-line start-col)
                  (Err (+ "unknown escape \\" (+ esc (+ " at line:" (+ "" line)))))))))))
            ;; Regular character
            (if (= ch "\n")
              (lex-string source (+ pos 1) (+ line 1) 0 (+ acc ch) start-line start-col)
              (lex-string source (+ pos 1) line (+ col 1) (+ acc ch) start-line start-col)))))))
```

- [ ] **Step 2: Add string tests**

```clojure
;; ── String tests ────────────────────────────────────

(do
  ;; Simple string
  (match (lex "\"hello\"")
    (Ok tokens) (assert-eq (at tokens 0) (Token "string" "hello" 1 0))
    (Err msg) (print "FAIL:" msg))

  ;; Empty string
  (match (lex "\"\"")
    (Ok tokens) (assert-eq (at tokens 0) (Token "string" "" 1 0))
    (Err msg) (print "FAIL:" msg))

  ;; String with escape
  (match (lex "\"a\\nb\"")
    (Ok tokens) (assert-eq (at tokens 0) (Token "string" "a\nb" 1 0))
    (Err msg) (print "FAIL:" msg))

  ;; Unterminated string
  (match (lex "\"oops")
    (Ok _) (print "FAIL: should have errored")
    (Err msg) (assert-eq (contains msg "unterminated") true))

  (print "string tests passed"))
```

- [ ] **Step 3: Run tests**

Run: `cargo run -- run bootstrap/lexer_test.airl`
Expected: prints "string tests passed"

- [ ] **Step 4: Commit**

```bash
git add bootstrap/lexer.airl bootstrap/lexer_test.airl
git commit -m "feat(bootstrap): add string sub-lexer with escape handling"
```

---

### Task 7: Integration Tests and Rust Fixture

**Files:**
- Modify: `bootstrap/lexer_test.airl`
- Create: `tests/fixtures/valid/lexer_bootstrap.airl`

Add end-to-end tests that lex complete AIRL expressions and a Rust fixture test.

- [ ] **Step 1: Add integration tests to `lexer_test.airl`**

```clojure
;; ── Integration tests ───────────────────────────────

(do
  ;; Complete expression
  (match (lex "(+ 1 2)")
    (Ok tokens) (do
      (assert-eq (length tokens) 6)
      (assert-eq (at tokens 0) (Token "lparen" "(" 1 0))
      (assert-eq (at tokens 1) (Token "symbol" "+" 1 1))
      (assert-eq (at tokens 2) (Token "integer" 1 1 3))
      (assert-eq (at tokens 3) (Token "integer" 2 1 5))
      (assert-eq (at tokens 4) (Token "rparen" ")" 1 6))
      (assert-eq (at tokens 5) (Token "eof" nil 1 7)))
    (Err msg) (print "FAIL:" msg))

  ;; Function definition
  (match (lex "(defn add :sig [(a : i32) -> i32] :body (+ a 1))")
    (Ok tokens) (do
      (assert-eq (at tokens 0) (Token "lparen" "(" 1 0))
      (assert-eq (at tokens 1) (Token "symbol" "defn" 1 1))
      (assert-eq (at tokens 2) (Token "symbol" "add" 1 6))
      (assert-eq (at tokens 3) (Token "keyword" "sig" 1 10)))
    (Err msg) (print "FAIL:" msg))

  ;; Multi-line with comments
  (match (lex ";; comment\n(+ 1 2)")
    (Ok tokens) (do
      (assert-eq (at tokens 0) (Token "lparen" "(" 2 0))
      (assert-eq (at tokens 1) (Token "symbol" "+" 2 1)))
    (Err msg) (print "FAIL:" msg))

  ;; Lex an actual file (uses read-file builtin)
  (match (lex (read-file "bootstrap/lexer_test.airl"))
    (Ok tokens) (do
      (print "self-lex token count:" (length tokens))
      (assert-eq (> (length tokens) 100) true))
    (Err msg) (print "FAIL: could not lex own source:" msg))

  (print "integration tests passed")
  (print "ALL TESTS PASSED"))
```

- [ ] **Step 2: Create Rust fixture test**

Create `tests/fixtures/valid/lexer_bootstrap.airl`:

```clojure
;; EXPECT: true
;; Smoke test for the builtins the self-hosted lexer depends on.
;; The full lexer tests are in bootstrap/lexer_test.airl (run via cargo run -- run).
;; Fixtures can't :load external files, so we test the underlying primitives here.
(do
  (let (src : String "(+ 1 2)")
    (let (ch : String (char-at src 0))
      (let (is-paren : Bool (= ch "("))
        (let (is-digit : Bool (contains "0123456789" (char-at src 3)))
          (let (digit-val : i64 (index-of "0123456789" (char-at src 3)))
            (let (sub : String (substring src 1 2))
              (and is-paren
                (and is-digit
                  (and (= digit-val 1)
                    (= sub "+")))))))))))
```

Note: The fixture can't `:load` the lexer since the fixture runner doesn't support that. This tests the builtins the lexer depends on (`char-at`, `contains`, `index-of`, `substring`). The real lexer tests are in `bootstrap/lexer_test.airl`.

- [ ] **Step 3: Run all tests**

Run: `cargo run -- run bootstrap/lexer_test.airl`
Expected: prints "ALL TESTS PASSED"

Run: `cargo test --workspace --exclude airl-mlir`
Expected: All tests pass including the new fixture

- [ ] **Step 4: Commit**

```bash
git add bootstrap/lexer.airl bootstrap/lexer_test.airl tests/fixtures/valid/lexer_bootstrap.airl
git commit -m "feat(bootstrap): add integration tests and Rust fixture for lexer"
```

---

### Task 8: Update CLAUDE.md and Final Verification

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update CLAUDE.md**

Add to the Completed Tasks section:
```
- **Self-Hosted Lexer** — `bootstrap/lexer.airl` (~15 functions) tokenizes AIRL source strings using index-walking recursion. Handles all token types, escape sequences, nested block comments, line/col tracking, and Result-based error propagation. Tested by `bootstrap/lexer_test.airl`.
```

Add to the project overview or a new section:
```
## Bootstrap Compiler

The self-hosted compiler lives in `bootstrap/`. Run tests with:
\`\`\`bash
cargo run -- run bootstrap/lexer_test.airl    # Lexer tests
\`\`\`
```

- [ ] **Step 2: Run full test suite**

Run: `cargo test --workspace --exclude airl-mlir`
Expected: All tests pass

Run: `cargo run -- run bootstrap/lexer_test.airl`
Expected: prints "ALL TESTS PASSED"

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: add self-hosted lexer to CLAUDE.md and bootstrap section"
```
