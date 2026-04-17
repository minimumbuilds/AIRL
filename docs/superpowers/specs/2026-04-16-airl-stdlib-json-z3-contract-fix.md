# AIRL Stdlib: json-skip-ws Z3 Contract Fix
**Date:** 2026-04-16  
**Status:** Draft  
**Scope:** AIRL stdlib (`/mnt/b6d8b397-9fc1-42ac-a0da-8664a73d4ee9/AIRL/stdlib/json.airl`) — external to AirVault  
**Blocks:** AirVault build (`build.sh`)

---

## Problem

AirVault fails to build with the following Z3 contract violation from the g3 AOT compiler:

```
Compile error: Z3 contract violation: function 'json-skip-ws' has a disproven contract
  counterexample: result -> (- 1)
                  pos -> 0
```

The error originates in the AIRL standard library at `stdlib/json.airl`, not in AirVault source code.

---

## Prior Fix (insufficient)

Commit `788ab10` in the AIRL repo weakened the `:ensures` clause from `(>= result pos)` to `(>= result 0)` for both `json-skip-ws` and `json-parse-number-end`. This is already applied in the current stdlib source. However, Z3 still disproves the contract — the weaker postcondition didn't resolve the underlying proof gap.

---

## Root Cause

The `json-skip-ws` function (line 108 of `stdlib/json.airl`) currently has:

```lisp
(defn json-skip-ws
  :sig [(src : Bytes) (pos : i64) (len : i64) -> i64]
  :requires [(>= pos 0)]
  :ensures [(>= result 0)]
  :body (if (>= pos len)
          pos
          (let (b : i64 (at src pos))
            (if (or (= b 32) (or (= b 9) (or (= b 10) (= b 13))))
              (json-skip-ws src (+ pos 1) len)
              pos))))
```

The function always returns `pos` — either directly when `pos >= len` or when the current byte is non-whitespace, or via recursion with `(+ pos 1)`. Since `pos` starts `>= 0` and only increments, `result >= 0` should always hold.

However, Z3 cannot prove this. The `len` parameter is **unconstrained** in the `:requires` clause. Without knowing `len >= 0`, Z3 cannot bound the recursion depth or verify the inductive step that the postcondition holds through all recursive calls. Z3 produces a spurious counterexample where `result = -1`.

---

## Fix

**File:** `stdlib/json.airl` (in the AIRL repo, not AirVault)

### 1. `json-skip-ws` (line 111)

```lisp
;; Before
:requires [(>= pos 0)]

;; After
:requires [(>= pos 0) (>= len 0)]
```

### 2. `json-parse-number-end` (line 204)

Same pattern, same fix:

```lisp
;; Before
:requires [(>= pos 0)]

;; After
:requires [(>= pos 0) (>= len 0)]
```

### 3. `json-has-dot` (line 218)

Proactively — same unconstrained `end` parameter:

```lisp
;; Before
:requires [(>= start 0)]

;; After
:requires [(>= start 0) (>= end 0)]
```

### Why these changes are safe

All callers pass `len`/`end` derived from `(length src)` or `json-parse-number-end` (which itself returns `>= 0`), which are always non-negative:

| Caller | How `len`/`end` is derived |
|--------|---------------------------|
| `json-skip-ws` (self-recursive) | Passes `len` unchanged |
| `json-parse-array-loop` | Passes `len` from caller chain |
| `json-parse-object-loop` | Passes `len` from caller chain |
| `json-parse-value` | Passes `len` from caller chain |
| `json-parse` (root) | `len = (length src)`, always >= 0 |
| `json-has-dot` | `end` comes from `json-parse-number-end`, result >= 0 |

No caller computes `len`/`end` from arithmetic that could go negative.

---

## Verification

After applying the fix in the AIRL repo, delete the stale Z3 cache and rebuild AirVault:

```bash
rm /mnt/b6d8b397-9fc1-42ac-a0da-8664a73d4ee9/AirVault/.g3-z3-cache
cd /mnt/b6d8b397-9fc1-42ac-a0da-8664a73d4ee9/AirVault && bash build.sh
```

All three functions should show `proven` in the new cache. The ~40 linearity warnings in `json.airl` are pre-existing and unrelated.

---

## Notes

- The stale `.g3-z3-cache` in the AirVault project directory must be deleted after the stdlib fix is applied, since it contains cached `disproven` results keyed by the old contract hashes.
- The linearity warnings throughout `json.airl` (ownership disagreements on `src`, `val`, `acc`) are a separate concern and not addressed here.
