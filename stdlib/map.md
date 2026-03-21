# AIRL Standard Library: Map (Dictionary)

> Source: `stdlib/map.airl` + 10 Rust builtins | 18 functions total | Auto-loaded

Hash map (dictionary) data type with string keys and arbitrary values. Backed by Rust's `HashMap<String, Value>` for O(1) lookups. Maps are immutable from AIRL's perspective — operations return new maps. All functions are available automatically — no imports needed.

## Dependencies

AIRL helpers depend on Collections (`map`, `fold`, `head`, `at`).

## Creating Maps

```lisp
;; Empty map
(map-new)                              ;; → {}

;; From flat key-value list
(map-from ["name" "AIRL" "version" 1]) ;; → {name: "AIRL", version: 1}

;; From list of [key value] pairs
(map-from-entries [["a" 1] ["b" 2]])   ;; → {a: 1, b: 2}

;; Build incrementally
(map-set (map-set (map-new) "x" 10) "y" 20)  ;; → {x: 10, y: 20}
```

## Reading Maps

```lisp
(let (m : _ (map-from ["name" "AIRL" "version" 1 "year" 2025]))
  (do
    (map-get m "name")            ;; → "AIRL"
    (map-get m "missing")         ;; → nil
    (map-get-or m "missing" "?")  ;; → "?"
    (map-has m "name")            ;; → true
    (map-has m "nope")            ;; → false
    (map-size m)                  ;; → 3
    (map-keys m)                  ;; → ["name" "version" "year"]
    (map-values m)                ;; → ["AIRL" 1 2025]
    (map-entries m)))             ;; → [["name" "AIRL"] ["version" 1] ["year" 2025]]
```

## Modifying Maps

All operations return new maps — the original is unchanged.

```lisp
(let (m : _ (map-from ["a" 1 "b" 2 "c" 3]))
  (do
    ;; Set (add or overwrite)
    (map-set m "d" 4)             ;; → {a: 1, b: 2, c: 3, d: 4}

    ;; Remove
    (map-remove m "b")            ;; → {a: 1, c: 3}

    ;; Update existing key with function
    (map-update m "a" (fn [v] (* v 10)))  ;; → {a: 10, b: 2, c: 3}

    ;; Update with default for missing keys
    (map-update-or m "z" 0 (fn [v] (+ v 1)))))  ;; → {a: 1, b: 2, c: 3, z: 1}
```

## Transforming Maps

```lisp
(let (m : _ (map-from ["a" 1 "b" 2 "c" 3]))
  (do
    ;; Double all values
    (map-map-values (fn [v] (* v 2)) m)
    ;; → {a: 2, b: 4, c: 6}

    ;; Keep only values > 1
    (map-filter (fn [k v] (> v 1)) m)
    ;; → {b: 2, c: 3}

    ;; Count entries matching predicate
    (map-count (fn [k v] (> v 1)) m)))
    ;; → 2
```

## Merging Maps

```lisp
(let (m1 : _ (map-from ["a" 1 "b" 2]))
  (let (m2 : _ (map-from ["b" 99 "c" 3]))
    (map-merge m1 m2)))
;; → {a: 1, b: 99, c: 3}  (m2 wins on conflict)
```

## Builtin Function Reference (Rust)

| Function | Signature | Returns | Description |
|----------|-----------|---------|-------------|
| `map-new` | `(map-new)` | Map | Create empty map |
| `map-from` | `(map-from [k1 v1 k2 v2 ...])` | Map | Create from flat key-value list. Keys must be strings |
| `map-get` | `(map-get m key)` | any/nil | Get value, or nil if missing |
| `map-get-or` | `(map-get-or m key default)` | any | Get value, or default if missing |
| `map-set` | `(map-set m key value)` | Map | Return new map with key set |
| `map-has` | `(map-has m key)` | Bool | Does key exist? |
| `map-remove` | `(map-remove m key)` | Map | Return new map with key removed |
| `map-keys` | `(map-keys m)` | List | All keys (sorted alphabetically) |
| `map-values` | `(map-values m)` | List | All values (in key-sorted order) |
| `map-size` | `(map-size m)` | Int | Number of entries |

## AIRL Helper Function Reference

| Function | Signature | Returns | Description |
|----------|-----------|---------|-------------|
| `map-entries` | `(map-entries m)` | List | All entries as `[[k v] ...]` pairs |
| `map-from-entries` | `(map-from-entries pairs)` | Map | Create from `[[k v] ...]` pairs |
| `map-merge` | `(map-merge m1 m2)` | Map | Merge maps, m2 wins on conflict |
| `map-map-values` | `(map-map-values f m)` | Map | Apply f to every value |
| `map-filter` | `(map-filter pred m)` | Map | Keep entries where `(pred key value)` is true |
| `map-update` | `(map-update m key f)` | Map | Apply f to value at key (no-op if missing) |
| `map-update-or` | `(map-update-or m key default f)` | Map | Apply f to value (or f(default) if missing) |
| `map-count` | `(map-count pred m)` | Int | Count entries matching predicate |

## Patterns

### Symbol Table for a Compiler

```lisp
;; Build a scope with variable bindings
(let (scope : _ (map-from ["x" 42 "y" 10]))
  ;; Lookup a variable
  (let (val : _ (map-get scope "x"))
    ;; Add a new binding
    (let (inner-scope : _ (map-set scope "z" 99))
      (print "x =" (map-get inner-scope "x")
             "z =" (map-get inner-scope "z")))))
```

### Frequency Counter

```lisp
;; Count character frequencies in a string
(defn char-freq
  :sig [(s : String) -> _]
  :requires [(valid s)]
  :ensures [(valid result)]
  :body (fold (fn [acc ch] (map-update-or acc ch 0 (fn [n] (+ n 1))))
              (map-new)
              (chars s)))

(char-freq "hello")  ;; → {e: 1, h: 1, l: 2, o: 1}
```

## Notes

- **Keys are always strings.** If you need non-string keys, convert with `int-to-string` (when available) or use the value's Display representation.
- **Maps are immutable.** `map-set` and `map-remove` return new maps — the original is unchanged. This fits AIRL's functional style.
- **O(1) operations:** `map-get`, `map-set`, `map-has`, `map-remove`, `map-size` are all O(1) amortized (backed by Rust HashMap).
- **Deterministic output:** `map-keys`, `map-values`, `map-entries`, and `Display` all sort by key for reproducible output.
- **`type-of` returns `"Map"`** for map values.
