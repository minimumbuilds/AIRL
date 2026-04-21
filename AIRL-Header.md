# AIRL Reference Header

## TRAPS

1. NO loops/mutation: no `while`/`for`/`set!`/`var`/`return`. USE `fold`/`map`/`filter`/recursion.
2. `and`/`or` are EAGER and BINARY. Nest: `(or a (or b c))`. For short-circuit, use nested `if`.
3. `if` has EXACTLY 3 forms: `(if cond then else)`. Both branches required. Multi-expr branch: wrap in `do`.
4. No mixed int/float: `(+ 1 1.0)` errors. Use `(+ 1.0 1.0)` or `int-to-float`.
5. `let` REQUIRES type annotation AND body: `(let (x : i64 5) body)`. No body = parse error.
6. `let` supports multi-binding: `(let (x : T v1) (y : T v2) body)`. PREFER flat multi-binding over nested single lets.
7. Every `defn` needs `:sig` + `:body` + at least one of `:requires`/`:ensures`. Min guard: `(valid x)`. Use `define` for quick functions without contracts.
8. `result` only available in `:ensures`/`:invariant`, NOT `:requires`.
9. `match` arms are FLAT pairs after scrutinee: `(match expr pat1 body1 pat2 body2)`. Must be even count.
10. Variant constructors UPPERCASE: `(Ok 42)` not `(ok 42)`.
11. `empty?` is LIST ONLY. String empty check: `(= s "")`. `empty?` on string = runtime error.
12. Lambda params have NO types: `(fn [x y] body)` not `(fn [(x : i32)] body)`.
13. Integer division: `(/ 10 3)` -> `3`. Float division: `(/ 10.0 3.0)`.
14. Keywords are strings: `:foo` evaluates to `":foo"`.
15. `length` on strings = BYTE count. Unicode char count: `(char-count s)`.
16. Map keys are STRING ONLY. Use `int-to-string` for numeric keys. Same for Set elements.
17. Tensors: `(tensor.zeros [3 3])` no element type arg. `(tensor.rand [3 3] 42)` requires seed.
18. Use `char-code`/`char-from-code`, NOT `ord`/`chr` (don't exist).
19. These do NOT exist: `nil?` `null?` `list` `try` `catch` `throw` `typeof` `instanceof` `import` `require` `begin` `progn`.

## SYNTAX

### Forms
```
(defn NAME :sig [PARAMS -> RET] :requires [CONDS] :ensures [CONDS] :body EXPR)
  optional: :intent "desc" :invariant [CONDS] :execute-on cpu|gpu :priority normal
(define NAME (p1 p2 ...) BODY)           ;; lightweight: no contracts, no types, no :pub
(let (N1 : T1 V1) (N2 : T2 V2) ... BODY) ;; multi-binding (preferred)
(let (NAME : TYPE VAL) BODY)               ;; single-binding (special case)
(if COND THEN ELSE)
(do E1 E2 ... EN)                        ;; returns last
(match EXPR PAT1 BODY1 PAT2 BODY2 ...)   ;; flat pairs
(fn [p1 p2 ...] BODY)                    ;; lambda, no type annotations
(try EXPR)                               ;; unwrap Ok or propagate Err as runtime error
```

### Types
Integers: `i8`/`i16`/`i32`/`i64` (all i64 at runtime). Unsigned: `u8`-`u64` (all u64).
Floats: `f16`/`f32`/`f64`/`bf16` (all f64). Others: `Bool` `String` `Nil` `List` `Map` `fn` `tensor`.
`Result[T, E]` — variants `(Ok val)` `(Err reason)`. Wildcard: `_` (infer type).

### Patterns
`x` (bind) | `_` (wildcard) | `42`/`"hi"`/`true`/`nil` (literal) | `(Ok v)` (variant) | `(Ok (Some v))` (nested)

### Ownership
`(own x : T)` move | `(ref x : T)` immutable borrow | `(mut x : T)` mutable borrow | `(copy x : T)` copy | `(x : T)` default=own

### Contracts
`:requires [CONDS]` — checked before body. `:ensures [CONDS]` — checked after, `result` is bound.
`:invariant [CONDS]` — checked after body, before ensures. `(valid x)` — always true, minimal guard.

### deftype
```
(deftype Name :body (Sum (V1 [T]) (V2 [T T])))       ;; enum
(deftype Name :body (Product (field1 T) (field2 T)))   ;; struct
```

### Quantifiers
```
(forall [i : Nat] (where FILTER) BODY)   ;; runtime: iterate 0..9999, short-circuit on false
(exists [i : Nat] (where FILTER) BODY)   ;; runtime: iterate 0..9999, short-circuit on true
```

## SIGS

### Arithmetic (2-arg, same type, no mixed int/float)
```
(+ a b) -> same     ; Int/Float/Str. Str = concat
(- a b) -> same     ; Int/Float
(* a b) -> same     ; Int/Float
(/ a b) -> same     ; integer div for Int. DivByZero on 0
(% a b) -> same     ; remainder. DivByZero on 0
```

### Comparison (2-arg -> Bool, works on Int/Float/Str)
`=` `!=` `<` `>` `<=` `>=`

### Logic
```
(and a b) -> Bool    ; BINARY ONLY, EAGER (not short-circuit)
(or a b) -> Bool     ; BINARY ONLY, EAGER
(not x) -> Bool
(xor a b) -> Bool
```

### Bitwise (2-arg -> Int)
```
(bitwise-and a b) -> Int    ; AND
(bitwise-or a b) -> Int     ; OR
(bitwise-xor a b) -> Int    ; XOR
(bitwise-shl a n) -> Int    ; left shift
(bitwise-shr a n) -> Int    ; LOGICAL right shift (unsigned, no sign-extend)
```

### Collections (builtins)
```
(length xs) -> Int              ; list length or string BYTE length
(at xs i) -> elem               ; 0-based, errors on out-of-bounds
(append xs x) -> List           ; add to end
(head xs) -> elem               ; first element, errors on empty
(tail xs) -> List               ; all but first, errors on empty
(empty? xs) -> Bool             ; LIST ONLY — not for strings
(cons x xs) -> List             ; prepend
(at-or xs i default) -> elem    ; safe index, returns default on OOB
(set-at xs i v) -> List         ; immutable update at index
(list-contains? xs v) -> Bool
```

### Collections (stdlib, auto-loaded)
```
(map f xs) -> List              ; apply f to each elem, preserves length
(filter pred xs) -> List        ; keep where pred returns true
(fold f init xs) -> any         ; left fold: f(f(f(init,x1),x2),x3)
(reverse xs) -> List
(concat xs ys) -> List
(zip xs ys) -> List             ; [[x1 y1]...], stops at shorter list
(flatten xss) -> List           ; one level of nesting
(range start end) -> List       ; [start,end) integers. Requires start<=end
(take n xs) -> List             ; first n elems, clamps to available (no error)
(drop n xs) -> List             ; skip first n
(any pred xs) -> Bool           ; short-circuits on first true
(all pred xs) -> Bool           ; short-circuits on first false
(find pred xs) -> any/nil       ; first match or nil
(sort cmp xs) -> List           ; merge sort. cmp: (fn [a b] (< a b))
(merge cmp xs ys) -> List       ; merge two pre-sorted lists
(unique xs) -> List             ; dedup, preserves first occurrence
(enumerate xs) -> List          ; [[0 a] [1 b] ...]
(group-by f xs) -> Map          ; {(f x): [matching elems]}
```

### Math (stdlib, auto-loaded, integer-only)
```
(abs x) -> Int                  ; ensures >= 0
(min a b) -> Int
(max a b) -> Int
(clamp x lo hi) -> Int          ; requires lo<=hi
(sign x) -> Int                 ; -1, 0, or 1
(even? x) -> Bool
(odd? x) -> Bool
(pow base exp) -> Int           ; requires exp>=0
(gcd a b) -> Int                ; requires a>=0, b>=0
(lcm a b) -> Int
(sum-list xs) -> Int
(product-list xs) -> Int
```

### Float math (builtins, accept Int or Float via auto-coerce)
```
(sqrt x) (sin x) (cos x) (tan x) (log x) (exp x) -> Float
(floor x) (ceil x) (round x) -> Int     ; rounding
(float-to-int x) -> Int                 ; truncate toward zero
(int-to-float n) -> Float
(infinity) (nan) -> Float               ; 0-arg constructors
(is-nan? x) (is-infinite? x) -> Bool
```

### String (builtins)
```
(str a b ...) -> Str            ; VARIADIC concat, auto-coerces all types. Preferred over nested +
(char-at s i) -> Str            ; single char, Unicode-safe, errors on OOB
(substring s start end) -> Str  ; chars [start,end)
(chars s) -> List               ; list of single-char strings
(split s delim) -> List
(join xs sep) -> Str
(contains s sub) -> Bool
(starts-with s prefix) -> Bool
(ends-with s suffix) -> Bool
(index-of s sub) -> Int         ; char index, -1 if not found
(trim s) -> Str
(to-upper s) -> Str
(to-lower s) -> Str
(replace s old new) -> Str      ; all occurrences
(char-alpha? s) -> Bool         ; first char is Unicode alphabetic
(char-digit? s) -> Bool         ; first char is ASCII digit 0-9
(char-whitespace? s) -> Bool    ; first char is Unicode whitespace
(char-upper? s) -> Bool         ; first char is Unicode uppercase
(char-lower? s) -> Bool         ; first char is Unicode lowercase
(string-ci=? a b) -> Bool       ; case-insensitive equality (Unicode case fold)
```

### String (stdlib)
```
(words s) -> List               ; split by whitespace, no empty strings
(unwords ws) -> Str             ; join with spaces
(lines s) -> List               ; split by newlines
(unlines ls) -> Str             ; join with newlines
(repeat-str s n) -> Str         ; requires n>=0
(pad-left s width ch) -> Str
(pad-right s width ch) -> Str
(is-empty-str s) -> Bool
(reverse-str s) -> Str
(count-occurrences s sub) -> Int ; non-overlapping, requires non-empty sub
```

### Result (stdlib)
```
(is-ok? r) -> Bool
(is-err? r) -> Bool
(unwrap-or r default) -> any    ; extract Ok value, or return default
(map-ok f r) -> Result          ; apply f to Ok value, pass Err through
(map-err f r) -> Result         ; apply f to Err value, pass Ok through
(and-then f r) -> Result        ; f must return Result (monadic bind)
(or-else f r) -> Result         ; recover from Err: f receives the error
(ok-or val err) -> Result       ; nil->(Err err), non-nil->(Ok val)
```

### Map (builtins — keys are STRING ONLY)
```
(map-new) -> Map
(map-from [k1 v1 k2 v2 ...]) -> Map     ; flat key-value list, keys must be strings
(map-get m key) -> any/nil
(map-get-or m key default) -> any
(map-set m key val) -> Map               ; returns new map (immutable)
(map-has m key) -> Bool
(map-remove m key) -> Map
(map-keys m) -> List                     ; sorted alphabetically
(map-values m) -> List                   ; key-sorted order
(map-size m) -> Int
```

### Map (stdlib)
```
(map-entries m) -> List                  ; [[k v] ...] pairs
(map-from-entries pairs) -> Map          ; from [[k v] ...] pairs
(map-merge m1 m2) -> Map                ; m2 wins on conflict
(map-map-values f m) -> Map             ; apply f to every value
(map-filter pred m) -> Map              ; pred takes (key value)
(map-update m key f) -> Map             ; no-op if key missing
(map-update-or m key default f) -> Map  ; applies f to default if key missing
(map-count pred m) -> Int               ; pred takes (key value)
```

### Set (stdlib — elements must be strings, implemented over maps)
```
(set-new) -> Map  (set-from xs) -> Map  (set-add s x) -> Map  (set-remove s x) -> Map
(set-contains? s x) -> Bool  (set-size s) -> Int  (set-to-list s) -> List
(set-union a b) -> Map  (set-intersection a b) -> Map  (set-difference a b) -> Map
(set-subset? a b) -> Bool  (set-equal? a b) -> Bool
```

### Testing (stdlib, auto-loaded)
```
(assert-eq actual expected) -> Nil    ; panic if actual != expected
(assert-ne actual expected) -> Nil    ; panic if actual == expected
(assert-ok r) -> Nil                  ; panic if r is not (Ok ...)
(assert-err r) -> Nil                 ; panic if r is not (Err ...)
(assert-contains haystack needle) -> Nil  ; panic if needle not in haystack
(assert-true val) -> Nil              ; panic if val is not true
```

### Type conversion
```
(int-to-string n) -> Str    (float-to-string f) -> Str
(string-to-int s) -> Int    (string-to-float s) -> Float   ; panic on invalid input
(char-code s) -> Int         ; Unicode codepoint of first char
(char-from-code n) -> Str    ; codepoint to 1-char string
(parse-int-radix s base) -> Result[Int, Str]  ; parse string in base 2-36
(int-to-string-radix n base) -> Str           ; format int in base 2-36
(type-of x) -> Str           ; "Int", "Bool", "Map", "List", etc.
```

### Utility
```
(print ...) -> Nil              ; variadic, space-separated
(println ...) -> Nil            ; variadic + newline
(eprint ...) -> Nil             ; variadic, to stderr
(eprintln ...) -> Nil           ; variadic + newline, to stderr
(read-line) -> Str              ; read single line from stdin
(read-stdin) -> Str             ; read all of stdin
(format s ...) -> Str           ; {} placeholders
(valid x) -> Bool               ; always true (contract guard)
(char-count s) -> Int           ; Unicode char count (not byte length)
(exit code) -> !
(panic msg) -> !                ; halt with error message
(assert cond msg) -> Bool       ; panic if false
(fn-metadata f) -> Map          ; function metadata (name, sig, contracts)
```

### File I/O (all paths sandboxed: no absolute paths, no `..`)
```
(read-file p) -> Str  (write-file p content) -> Nil  (append-file p content) -> Nil  (read-lines p) -> List
(file-exists? p) -> Bool  (is-dir? p) -> Bool  (file-size p) -> Int
(delete-file p) -> Nil  (delete-dir p) -> Nil  (rename-file old new) -> Nil
(create-dir p) -> Nil  (read-dir p) -> List   ; sorted entries
(temp-file prefix) -> Str                      ; create temp file, return path
(temp-dir prefix) -> Str                       ; create temp dir, return path
(file-mtime p) -> Int                          ; modification time as epoch millis, -1 on error
```

### Path
```
(path-join parts...) -> Str  (path-parent p) -> Str  (path-filename p) -> Str
(path-extension p) -> Str  (is-absolute? p) -> Bool
```

### Regex
```
(regex-match pat s) -> Str/nil        ; first match or nil
(regex-find-all pat s) -> List        ; all matches
(regex-replace pat s replacement) -> Str
(regex-split pat s) -> List
```

### Crypto
```
(sha256 s) -> Str  (sha512 s) -> Str          ; hex digest of string
(hmac-sha256 key msg) -> Str  (hmac-sha512 key msg) -> Str  ; hex HMAC of strings
(sha256-bytes buf) -> IntList                  ; raw 32-byte hash of IntList
(sha512-bytes buf) -> IntList                  ; raw 64-byte hash of IntList
(hmac-sha256-bytes key data) -> IntList        ; raw HMAC of IntList inputs
(hmac-sha512-bytes key data) -> IntList        ; raw HMAC of IntList inputs
(pbkdf2-sha256 password salt iterations key-length) -> IntList  ; key derivation
(pbkdf2-sha512 password salt iterations key-length) -> IntList  ; key derivation
(base64-encode s) -> Str  (base64-decode s) -> Str             ; string base64
(base64-encode-bytes buf) -> Str               ; encode IntList to base64 string
(base64-decode-bytes s) -> IntList             ; decode base64 string to IntList
(random-bytes n) -> List              ; n random bytes (0-255)
```

### System
```
(shell-exec cmd args-list) -> Result[Map, Str]  ; Ok map: {"stdout" "stderr" "exit-code"}
(shell-exec-with-stdin cmd args-list stdin-str) -> Result[Map, Str]  ; pipe stdin to process
(time-now) -> Int                     ; epoch milliseconds
(sleep ms) -> Nil
(format-time ms fmt) -> Str           ; UTC. Supports %Y %m %d %H %M %S
(getenv name) -> Result[Str, Str]
(get-args) -> List                    ; command-line args as strings
(get-cwd) -> Str                      ; current working directory
(cpu-count) -> Int                    ; logical CPU count
(rt-stats label) -> Nil               ; print runtime alloc snapshot (diagnostic)
(bc-func-from name arity regs caps consts instrs) -> BCFunc  ; native bytecode-func ctor
(bc-func-is-main? bcf) -> Bool        ; true when name == "__main__"
(bc-func-name bcf) -> Str             ; diagnostic accessor
```

### Network/JSON
```
(json-parse s) -> any
(json-stringify v) -> Str
```

**HTTP:** Use the AIReqL library (`../AIReqL`). Provides a requests-like API over raw TCP:
```
(aireql-get url) -> Map                         ; simple GET
(aireql-post-with-opts url opts-map) -> Map     ; POST with body/headers
(aireql-request method url opts-map) -> Map     ; generic request
(aireql-status-code resp) -> Int                ; 200, 404, etc.
(aireql-text resp) -> Str                       ; response body
(aireql-json resp) -> any                       ; parse body as JSON
(aireql-ok? resp) -> Bool                       ; true if 2xx
```
Compile with: `g3 -- aireql-util.airl aireql-transport.airl aireql.airl aireql-session.airl your-app.airl`

### Thread (thread-per-task, message-passing only)
```
(thread-spawn closure) -> Int                       ; spawn thread running 0-arg closure, returns handle
(thread-join handle) -> Result[any, Str]            ; block until done. Ok(value) or Err(msg)
(thread-set-affinity core-id) -> Result[Nil, Str]   ; pin calling thread to CPU core (Linux only)
```

### Channel (unbounded, std::sync::mpsc)
```
(channel-new) -> [Int Int]                          ; returns [sender-handle receiver-handle]
(channel-send tx value) -> Result[Bool, Str]        ; send value. Err if closed
(channel-recv rx) -> Result[any, Str]               ; blocking recv. Err if closed
(channel-recv-timeout rx ms) -> Result[any, Str]    ; recv with timeout. ms=0 is non-blocking (try_recv)
(channel-drain rx) -> List                          ; drain all available messages without blocking
(channel-close handle) -> Bool                      ; close sender or receiver
```

### Bytes (big-endian, IntList = byte sequences)
```
(bytes-new) -> IntList                ; empty byte list
(bytes-from-int8 n) -> IntList        ; i8 to 1 byte
(bytes-from-int16 n) -> IntList       ; i16 to 2 bytes BE
(bytes-from-int32 n) -> IntList       ; i32 to 4 bytes BE
(bytes-from-int64 n) -> IntList       ; i64 to 8 bytes BE
(bytes-to-int16 buf offset) -> Int    ; read i16 from byte list at offset
(bytes-to-int32 buf offset) -> Int    ; read i32 from byte list at offset
(bytes-to-int64 buf offset) -> Int    ; read i64 from byte list at offset
(bytes-from-string s) -> IntList      ; UTF-8 encode string to bytes
(bytes-to-string buf offset len) -> Str ; UTF-8 decode bytes to string
(bytes-concat a b) -> IntList         ; concatenate two byte lists
(bytes-concat-all parts) -> IntList   ; concatenate List[IntList] in one O(n) pass
(bytes-slice buf offset len) -> IntList ; extract slice with bounds check
(crc32c buf) -> Int                   ; CRC32C checksum
```

### Compression (IntList in, IntList out)
```
(gzip-compress buf) -> IntList
(gzip-decompress buf) -> IntList
(snappy-compress buf) -> IntList
(snappy-decompress buf) -> IntList
(lz4-compress buf) -> IntList
(lz4-decompress buf) -> IntList
(zstd-compress buf) -> IntList
(zstd-decompress buf) -> IntList
```

### TCP (handle-based, all return Result)
```
(tcp-listen port backlog) -> Result[Int, Str]      ; bind + listen, returns server handle
(tcp-accept handle) -> Result[Int, Str]            ; blocking accept, returns connection handle
(tcp-accept-tls handle ca-path cert-path key-path) -> Result[Int, Str]
  ; server-side TLS accept. ca-path: CA cert PEM, cert-path/key-path: server cert/key PEM
(tcp-connect host port) -> Result[Int, Str]        ; connect, returns handle
(tcp-connect-tls host port ca-path cert-path key-path) -> Result[Int, Str]
  ; TLS connection. ca-path: CA cert PEM ("" = system roots via webpki-roots)
  ; cert-path/key-path: client cert/key PEM ("" = no client auth)
  ; returned handle works with tcp-send, tcp-recv, tcp-close
(tcp-close handle) -> Result[Nil, Str]             ; close connection or listener
(tcp-send handle data) -> Result[Int, Str]         ; send IntList, returns bytes sent
(tcp-recv handle max-bytes) -> Result[IntList, Str] ; recv up to max-bytes
(tcp-recv-exact handle n) -> Result[IntList, Str]  ; recv exactly n bytes or error
(tcp-set-timeout handle ms) -> Result[Nil, Str]    ; ms<=0 = no timeout
```

### Tensor (all f32 internally, shapes are [dim ...] int lists)
```
(tensor.zeros [dims])  (tensor.ones [dims])  (tensor.rand [dims] seed)  (tensor.identity n)
(tensor.add a b)  (tensor.mul a b)           ; element-wise, same shape
(tensor.matmul a b)  (tensor.transpose t)    ; matrix ops
(tensor.reshape t [shape])  (tensor.slice t start end)
(tensor.sum t) -> Float  (tensor.max t) -> Float  (tensor.softmax t) -> Tensor
```

### Agent
```
(spawn-agent "file.airl") -> Str             ; spawns child process, returns agent name
(send agent "fn-name" arg...) -> any         ; sync, blocks until result
(send-async agent "fn-name" arg...) -> Str   ; returns task-id immediately
(await task-id) -> any                       ; block until done
(await task-id timeout-ms) -> any            ; with timeout
(parallel [task-ids]) -> List                ; await multiple
(parallel [task-ids] timeout-ms) -> List
(broadcast [agents] "fn-name" arg...) -> any ; first success wins
(retry agent "fn-name" arg... :max n)        ; exponential backoff
(escalate agent :reason msg :data val)
(any-agent) -> Str                           ; first spawned agent name
```

### Compilation
```
(run-bytecode bc-funcs) -> any                           ; execute BCFunc list in bytecode VM
(compile-to-executable [paths] output) -> Nil            ; source files → native binary (Rust pipeline)
(compile-bytecode-to-executable bc-funcs output) -> Str  ; BCFunc list → native binary (G3 pipeline)
(compile-bytecode-to-executable-with-target bc-funcs output target) -> Str  ; with target triple
```
CLI cross-compilation: `airl compile file.airl --target i686-airlos -o output` (or `--target x86_64-airlos`)
Targets: `x86-64` (default), `i686`, `i686-airlos` (freestanding), `x86_64-airlos` (freestanding 64-bit), `aarch64`

## PATTERNS

### Loop with accumulator -> fold
```
;; sum, product, count, build-string — any "running value" pattern
(fold (fn [acc x] (+ acc x)) 0 xs)
```

### Loop building list from indices -> map + range
```
(map (fn [i] (* i i)) (range 0 n))   ;; squares [0, n)
```

### Loop with counter -> fold
```
(fold (fn [count x] (if (pred x) (+ count 1) count)) 0 xs)
```

### While with two pointers -> recursion
```
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

### Nested match for Result chaining -> and-then
```
(and-then (fn [x] (safe-op x)) (first-op input))
```

### Frequency counter -> fold + map-update-or
```
(fold (fn [acc x] (map-update-or acc x 0 (fn [n] (+ n 1)))) (map-new) xs)
```
