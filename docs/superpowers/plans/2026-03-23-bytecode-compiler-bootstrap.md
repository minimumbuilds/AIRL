# Bootstrap Compiler → Bytecode Emission Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite `bootstrap/compiler.airl` to emit bytecode instructions (consumed by a new `run-bytecode` builtin) instead of IR nodes (consumed by `run-ir`), enabling v0.2 Phase 3 of the execution consolidation.

**Architecture:** The AIRL bootstrap compiler will produce a list-encoded bytecode program: a list of `(BCFunc name arity reg_count capture_count [constants...] [instructions...])` tuples. A new Rust builtin `run-bytecode` unmarshals this into `BytecodeFunc` structs, loads them into a `BytecodeVm`, and calls `__main__`. The compiler threads immutable state (registers, constants, locals, instructions) through all compilation functions, following the same pattern as the bootstrap lexer/parser.

**Tech Stack:** AIRL (bootstrap compiler), Rust (run-bytecode builtin), Cranelift JIT (unaffected)

**Reference files:**
- Current compiler: `bootstrap/compiler.airl` (~220 lines, emits IR)
- Bytecode format: `crates/airl-runtime/src/bytecode.rs` (Op enum, Instruction, BytecodeFunc)
- Rust bytecode compiler (reference): `crates/airl-runtime/src/bytecode_compiler.rs`
- Bytecode VM: `crates/airl-runtime/src/bytecode_vm.rs`
- Existing `run-ir` builtin: `crates/airl-runtime/src/builtins.rs:1087-1099`
- Existing equivalence test: `bootstrap/equivalence_test.airl`
- AIRL language guide: `AIRL-LLM-Guide.md`
- Stdlib docs: `stdlib/*.md`

**IMPORTANT:** Before writing or modifying any `.airl` file, you MUST read `AIRL-LLM-Guide.md` and all `stdlib/*.md` files completely. No exceptions.

---

## File Structure

| File | Action | Purpose |
|------|--------|---------|
| `crates/airl-runtime/src/builtins.rs` | Modify | Add `run-bytecode` builtin registration |
| `crates/airl-runtime/src/bytecode_marshal.rs` | Create | Marshal `Value::List` → `BytecodeFunc` structs |
| `crates/airl-runtime/src/lib.rs` | Modify | Add `pub mod bytecode_marshal;` |
| `bootstrap/bc_compiler.airl` | Create | New bytecode-emitting compiler |
| `bootstrap/bc_compiler_test.airl` | Create | Unit tests for bytecode compiler |
| `bootstrap/bc_equivalence_test.airl` | Create | Equivalence test: bytecode vs interpreter |

We create `bc_compiler.airl` as a NEW file (not modify `compiler.airl`) so the old IR compiler remains available as a reference during development. Once fixpoint is proven, `compiler.airl` can be deleted.

---

### Task 1: `run-bytecode` Builtin (Rust Side)

**Files:**
- Create: `crates/airl-runtime/src/bytecode_marshal.rs`
- Modify: `crates/airl-runtime/src/builtins.rs:155-156` (register_ir)
- Modify: `crates/airl-runtime/src/lib.rs`

The `run-bytecode` builtin takes a list of BCFunc representations from AIRL and executes them on the BytecodeVm. This is the bridge between the AIRL compiler's output and the Rust runtime.

**Bytecode serialization format from AIRL:**
```lisp
;; A program is a list of BCFunc values:
[(BCFunc "add" 2 4 0      ;; name, arity, reg_count, capture_count
   ["+"]                   ;; constants pool
   [(0 2 0 1)              ;; (op_code dst a b) — Add dst=2, a=0, b=1
    (4 0 2 0)])             ;; Return _, src=2, _
 (BCFunc "__main__" 0 3 0
   ["add" 1 2]
   [(9 1 0 0)              ;; LoadConst dst=1, const_idx=0
    (9 2 1 0)              ;; LoadConst dst=2, const_idx=1
    (21 0 0 2)             ;; Call dst=0, func_idx=0, argc=2
    (4 0 0 0)])]           ;; Return _, src=0, _
```

Op codes are encoded as integers matching the `Op` enum discriminant order (LoadConst=0, LoadNil=1, ...).

- [ ] **Step 1: Create `bytecode_marshal.rs` with value→BytecodeFunc conversion**

```rust
// crates/airl-runtime/src/bytecode_marshal.rs
//! Marshal AIRL Value representations of bytecode into BytecodeFunc structs.

use crate::bytecode::{BytecodeFunc, Instruction, Op};
use crate::value::Value;
use crate::error::RuntimeError;

/// Convert a Value::Variant("BCFunc", ...) into a BytecodeFunc.
pub fn value_to_bytecode_func(val: &Value) -> Result<BytecodeFunc, RuntimeError> {
    // Expect: (BCFunc name arity reg_count capture_count constants instructions)
    // which is Variant("BCFunc", List[name, arity, reg_count, capture_count, constants, instructions])
    match val {
        Value::Variant(tag, inner) if tag == "BCFunc" => {
            let fields = match inner.as_ref() {
                Value::List(items) => items,
                _ => return Err(RuntimeError::TypeError(
                    "BCFunc inner must be a list".into())),
            };
            if fields.len() != 6 {
                return Err(RuntimeError::TypeError(
                    format!("BCFunc expects 6 fields, got {}", fields.len())));
            }
            let name = match &fields[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(RuntimeError::TypeError("BCFunc name must be string".into())),
            };
            let arity = value_to_u16(&fields[1], "arity")?;
            let reg_count = value_to_u16(&fields[2], "reg_count")?;
            let capture_count = value_to_u16(&fields[3], "capture_count")?;
            let constants = match &fields[4] {
                Value::List(items) => items.clone(),
                _ => return Err(RuntimeError::TypeError("BCFunc constants must be list".into())),
            };
            let instructions = match &fields[5] {
                Value::List(items) => items.iter()
                    .map(value_to_instruction)
                    .collect::<Result<Vec<_>, _>>()?,
                _ => return Err(RuntimeError::TypeError(
                    "BCFunc instructions must be list".into())),
            };
            Ok(BytecodeFunc { name, arity, register_count: reg_count, capture_count, instructions, constants })
        }
        _ => Err(RuntimeError::TypeError(
            format!("expected BCFunc variant, got {}", val))),
    }
}

/// Convert a Value (list of 4 ints) into an Instruction.
fn value_to_instruction(val: &Value) -> Result<Instruction, RuntimeError> {
    match val {
        Value::List(items) if items.len() == 4 => {
            let op_num = value_to_u16(&items[0], "op")?;
            let dst = value_to_u16(&items[1], "dst")?;
            let a = value_to_u16(&items[2], "a")?;
            let b = value_to_u16(&items[3], "b")?;
            let op = int_to_op(op_num)?;
            Ok(Instruction::new(op, dst, a, b))
        }
        _ => Err(RuntimeError::TypeError(
            format!("instruction must be list of 4 ints, got {}", val))),
    }
}

fn value_to_u16(val: &Value, field: &str) -> Result<u16, RuntimeError> {
    match val {
        Value::Int(n) => Ok(*n as u16),
        _ => Err(RuntimeError::TypeError(format!("{} must be int, got {}", field, val))),
    }
}

fn int_to_op(n: u16) -> Result<Op, RuntimeError> {
    // Must match the order in the Op enum exactly
    match n {
        0 => Ok(Op::LoadConst),
        1 => Ok(Op::LoadNil),
        2 => Ok(Op::LoadTrue),
        3 => Ok(Op::LoadFalse),
        4 => Ok(Op::Move),
        5 => Ok(Op::Add),
        6 => Ok(Op::Sub),
        7 => Ok(Op::Mul),
        8 => Ok(Op::Div),
        9 => Ok(Op::Mod),
        10 => Ok(Op::Eq),
        11 => Ok(Op::Ne),
        12 => Ok(Op::Lt),
        13 => Ok(Op::Le),
        14 => Ok(Op::Gt),
        15 => Ok(Op::Ge),
        16 => Ok(Op::Not),
        17 => Ok(Op::Neg),
        18 => Ok(Op::Jump),
        19 => Ok(Op::JumpIfFalse),
        20 => Ok(Op::JumpIfTrue),
        21 => Ok(Op::Call),
        22 => Ok(Op::CallBuiltin),
        23 => Ok(Op::CallReg),
        24 => Ok(Op::TailCall),
        25 => Ok(Op::Return),
        26 => Ok(Op::MakeList),
        27 => Ok(Op::MakeVariant),
        28 => Ok(Op::MakeVariant0),
        29 => Ok(Op::MakeClosure),
        30 => Ok(Op::MatchTag),
        31 => Ok(Op::JumpIfNoMatch),
        32 => Ok(Op::MatchWild),
        33 => Ok(Op::TryUnwrap),
        _ => Err(RuntimeError::TypeError(format!("invalid opcode {}", n))),
    }
}

/// Run a bytecode program represented as a list of BCFunc values.
pub fn run_bytecode_program(funcs: &[Value]) -> Result<Value, RuntimeError> {
    use crate::bytecode_vm::BytecodeVm;
    let mut vm = BytecodeVm::new();
    for val in funcs {
        let func = value_to_bytecode_func(val)?;
        vm.load_function(func);
    }
    vm.exec_main()
}
```

- [ ] **Step 2: Register `run-bytecode` builtin in `builtins.rs`**

In `crates/airl-runtime/src/builtins.rs`, add after `register_ir`:

```rust
fn register_bytecode(&mut self) {
    self.register("run-bytecode", builtin_run_bytecode);
}
```

Call it from `Builtins::new()`. Add the function:

```rust
fn builtin_run_bytecode(args: &[Value]) -> Result<Value, RuntimeError> {
    expect_arity("run-bytecode", args, 1)?;
    let func_list = match &args[0] {
        Value::List(items) => items.clone(),
        _ => return Err(RuntimeError::TypeError("run-bytecode: expected list of BCFunc".into())),
    };
    crate::bytecode_marshal::run_bytecode_program(&func_list)
}
```

- [ ] **Step 3: Add module declaration to `lib.rs`**

Add `pub mod bytecode_marshal;` to `crates/airl-runtime/src/lib.rs`.

- [ ] **Step 4: Build and test manually**

```bash
source "$HOME/.cargo/env" && cargo build --release --features jit -p airl-driver
```

Create a minimal test AIRL file:
```bash
# Test: manually create a BCFunc and run it
cat > /tmp/test_run_bc.airl << 'EOF'
;; Manual bytecode: prints 42
;; __main__: LoadConst r0 <- 42, CallBuiltin "print" 1 arg at r0+1, Return r0
(let (prog : List
  [(BCFunc "__main__" 0 3 0
     [42 "print"]
     [(0 1 0 0)    ;; LoadConst dst=1, const_idx=0 (42)
      (4 0 1 0)    ;; Move dst=0+1=1 already there...
      ;; Actually: CallBuiltin dst=0, name_idx=1 ("print"), argc=1
      ;; args at r0+1 = r1 = 42
      (22 0 1 1)   ;; CallBuiltin dst=0, a=1(name_idx), b=1(argc)
      (25 0 0 0)]  ;; Return _, src=0
    )])
  (run-bytecode prog))
EOF
RUST_MIN_STACK=67108864 target/release/airl-driver run /tmp/test_run_bc.airl
```

Expected: prints `42`

- [ ] **Step 5: Run existing tests to confirm no regressions**

```bash
source "$HOME/.cargo/env" && RUST_MIN_STACK=67108864 cargo test --workspace --exclude airl-mlir --features jit 2>&1 | grep "test result:"
```

- [ ] **Step 6: Commit**

```bash
git add crates/airl-runtime/src/bytecode_marshal.rs crates/airl-runtime/src/builtins.rs crates/airl-runtime/src/lib.rs
git commit -m "feat: add run-bytecode builtin for bootstrap compiler bytecode execution"
```

---

### Task 2: Bootstrap Bytecode Compiler — Core Infrastructure

**Files:**
- Create: `bootstrap/bc_compiler.airl`

This task implements the compiler state management and literal/variable compilation. The compiler state is a map threaded through all functions:

```lisp
;; Compiler state is a map with keys:
;; "instrs"    — list of instructions (each is [op dst a b])
;; "consts"    — list of constants
;; "locals"    — map of variable name -> register number
;; "next-reg"  — next free register (int)
;; "max-reg"   — high water mark (int)
;; "lambdas"   — list of compiled lambda BCFunc values
;; "lambda-ctr" — counter for unique lambda names
```

- [ ] **Step 1: Write compiler state helpers**

Create `bootstrap/bc_compiler.airl` with:

```lisp
;; bootstrap/bc_compiler.airl — Bootstrap bytecode compiler
;; Requires: bootstrap/lexer.airl, bootstrap/parser.airl (loaded first)
;;
;; Compiles AST nodes to bytecode instructions (consumed by run-bytecode).

;; ── Op codes (must match Rust Op enum order) ────────────

(defn OP-LOAD-CONST  :sig [-> Int] :requires [true] :ensures [(valid result)] :body 0)
(defn OP-LOAD-NIL    :sig [-> Int] :requires [true] :ensures [(valid result)] :body 1)
(defn OP-LOAD-TRUE   :sig [-> Int] :requires [true] :ensures [(valid result)] :body 2)
(defn OP-LOAD-FALSE  :sig [-> Int] :requires [true] :ensures [(valid result)] :body 3)
(defn OP-MOVE        :sig [-> Int] :requires [true] :ensures [(valid result)] :body 4)
(defn OP-ADD         :sig [-> Int] :requires [true] :ensures [(valid result)] :body 5)
(defn OP-SUB         :sig [-> Int] :requires [true] :ensures [(valid result)] :body 6)
(defn OP-MUL         :sig [-> Int] :requires [true] :ensures [(valid result)] :body 7)
(defn OP-DIV         :sig [-> Int] :requires [true] :ensures [(valid result)] :body 8)
(defn OP-MOD         :sig [-> Int] :requires [true] :ensures [(valid result)] :body 9)
(defn OP-EQ          :sig [-> Int] :requires [true] :ensures [(valid result)] :body 10)
(defn OP-NE          :sig [-> Int] :requires [true] :ensures [(valid result)] :body 11)
(defn OP-LT          :sig [-> Int] :requires [true] :ensures [(valid result)] :body 12)
(defn OP-LE          :sig [-> Int] :requires [true] :ensures [(valid result)] :body 13)
(defn OP-GT          :sig [-> Int] :requires [true] :ensures [(valid result)] :body 14)
(defn OP-GE          :sig [-> Int] :requires [true] :ensures [(valid result)] :body 15)
(defn OP-NOT         :sig [-> Int] :requires [true] :ensures [(valid result)] :body 16)
(defn OP-NEG         :sig [-> Int] :requires [true] :ensures [(valid result)] :body 17)
(defn OP-JUMP        :sig [-> Int] :requires [true] :ensures [(valid result)] :body 18)
(defn OP-JUMP-IF-FALSE :sig [-> Int] :requires [true] :ensures [(valid result)] :body 19)
(defn OP-JUMP-IF-TRUE  :sig [-> Int] :requires [true] :ensures [(valid result)] :body 20)
(defn OP-CALL        :sig [-> Int] :requires [true] :ensures [(valid result)] :body 21)
(defn OP-CALL-BUILTIN :sig [-> Int] :requires [true] :ensures [(valid result)] :body 22)
(defn OP-CALL-REG    :sig [-> Int] :requires [true] :ensures [(valid result)] :body 23)
(defn OP-TAIL-CALL   :sig [-> Int] :requires [true] :ensures [(valid result)] :body 24)
(defn OP-RETURN      :sig [-> Int] :requires [true] :ensures [(valid result)] :body 25)
(defn OP-MAKE-LIST   :sig [-> Int] :requires [true] :ensures [(valid result)] :body 26)
(defn OP-MAKE-VARIANT :sig [-> Int] :requires [true] :ensures [(valid result)] :body 27)
(defn OP-MAKE-VARIANT0 :sig [-> Int] :requires [true] :ensures [(valid result)] :body 28)
(defn OP-MAKE-CLOSURE :sig [-> Int] :requires [true] :ensures [(valid result)] :body 29)
(defn OP-MATCH-TAG   :sig [-> Int] :requires [true] :ensures [(valid result)] :body 30)
(defn OP-JUMP-IF-NO-MATCH :sig [-> Int] :requires [true] :ensures [(valid result)] :body 31)
(defn OP-MATCH-WILD  :sig [-> Int] :requires [true] :ensures [(valid result)] :body 32)
(defn OP-TRY-UNWRAP  :sig [-> Int] :requires [true] :ensures [(valid result)] :body 33)

;; ── Compiler state constructors ─────────────────────────

(defn make-compiler-state
  :sig [-> Any]
  :intent "Create empty compiler state"
  :requires [true]
  :ensures [(valid result)]
  :body (map-from [["instrs" []]
                   ["consts" []]
                   ["locals" (map-new)]
                   ["next-reg" 0]
                   ["max-reg" 0]
                   ["lambdas" []]
                   ["lambda-ctr" 0]]))

;; State accessors
(defn cs-instrs   :sig [(s : Any) -> List] :requires [(valid s)] :ensures [(valid result)] :body (map-get s "instrs"))
(defn cs-consts   :sig [(s : Any) -> List] :requires [(valid s)] :ensures [(valid result)] :body (map-get s "consts"))
(defn cs-locals   :sig [(s : Any) -> Any]  :requires [(valid s)] :ensures [(valid result)] :body (map-get s "locals"))
(defn cs-next-reg :sig [(s : Any) -> Int]  :requires [(valid s)] :ensures [(valid result)] :body (map-get s "next-reg"))
(defn cs-max-reg  :sig [(s : Any) -> Int]  :requires [(valid s)] :ensures [(valid result)] :body (map-get s "max-reg"))
(defn cs-lambdas  :sig [(s : Any) -> List] :requires [(valid s)] :ensures [(valid result)] :body (map-get s "lambdas"))
(defn cs-lambda-ctr :sig [(s : Any) -> Int] :requires [(valid s)] :ensures [(valid result)] :body (map-get s "lambda-ctr"))

;; ── State mutators (return new state) ───────────────────

(defn cs-emit
  :sig [(s : Any) (op : Int) (dst : Int) (a : Int) (b : Int) -> Any]
  :intent "Append an instruction to the state"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body (map-set s "instrs" (append (cs-instrs s) [op dst a b])))

(defn cs-add-const
  :sig [(s : Any) (val : Any) -> List]
  :intent "Add constant, return [new-state index]"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body (let (consts : List (cs-consts s))
    (let (idx : Int (length consts))
      [(map-set s "consts" (append consts val)) idx])))

(defn cs-alloc-reg
  :sig [(s : Any) -> List]
  :intent "Allocate a register, return [new-state reg-number]"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body (let (r : Int (cs-next-reg s))
    (let (nr : Int (+ r 1))
      (let (new-max : Int (if (> nr (cs-max-reg s)) nr (cs-max-reg s)))
        [(map-set (map-set s "next-reg" nr) "max-reg" new-max) r]))))

(defn cs-alloc-regs
  :sig [(s : Any) (n : Int) -> List]
  :intent "Allocate n consecutive registers, return [new-state first-reg]"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body (let (r : Int (cs-next-reg s))
    (let (nr : Int (+ r n))
      (let (new-max : Int (if (> nr (cs-max-reg s)) nr (cs-max-reg s)))
        [(map-set (map-set s "next-reg" nr) "max-reg" new-max) r]))))

(defn cs-free-reg-to
  :sig [(s : Any) (r : Int) -> Any]
  :intent "Free registers above r"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body (map-set s "next-reg" r))

(defn cs-set-local
  :sig [(s : Any) (name : String) (reg : Int) -> Any]
  :intent "Bind variable name to register"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body (map-set s "locals" (map-set (cs-locals s) name reg)))

(defn cs-remove-local
  :sig [(s : Any) (name : String) -> Any]
  :intent "Remove variable binding"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body (map-set s "locals" (map-remove (cs-locals s) name)))

(defn cs-lookup-local
  :sig [(s : Any) (name : String) -> Any]
  :intent "Look up variable, return register or nil"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body (map-get-or (cs-locals s) name nil))

(defn cs-instr-count
  :sig [(s : Any) -> Int]
  :intent "Number of instructions emitted so far"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body (length (cs-instrs s)))

(defn cs-patch-jump
  :sig [(s : Any) (instr-idx : Int) (field : String) (value : Int) -> Any]
  :intent "Patch a jump instruction's offset field"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body (let (instrs : List (cs-instrs s))
    (let (instr : List (at instrs instr-idx))
      (let (patched : List
        (if (= field "a")
          [(at instr 0) (at instr 1) value (at instr 3)]
          [(at instr 0) (at instr 1) (at instr 2) value]))
        ;; Replace instruction at index — build new list
        (let (before : List (take instr-idx instrs))
          (let (after : List (drop (+ instr-idx 1) instrs))
            (map-set s "instrs" (concat before (cons patched after)))))))))

(defn cs-add-lambda
  :sig [(s : Any) (func : Any) -> Any]
  :intent "Add a compiled lambda function to the lambdas list"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body (map-set s "lambdas" (append (cs-lambdas s) func)))

(defn cs-next-lambda-name
  :sig [(s : Any) (prefix : String) -> List]
  :intent "Generate unique lambda name, return [new-state name]"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body (let (ctr : Int (cs-lambda-ctr s))
    [(map-set s "lambda-ctr" (+ ctr 1))
     (join [prefix "_lambda_" (+ "" ctr)] "")]))
```

- [ ] **Step 2: Build and verify helpers compile**

```bash
source "$HOME/.cargo/env" && cargo build --release --features jit -p airl-driver
# Quick smoke test — just load the file (helpers are defn's, don't run code)
echo '(print "loaded")' >> /tmp/bc_smoke.airl
cat bootstrap/lexer.airl bootstrap/parser.airl bootstrap/bc_compiler.airl /tmp/bc_smoke.airl > /tmp/bc_full_smoke.airl
RUST_MIN_STACK=67108864 target/release/airl-driver run /tmp/bc_full_smoke.airl
```

Expected: `"loaded"`

- [ ] **Step 3: Commit**

```bash
git add bootstrap/bc_compiler.airl
git commit -m "feat(bootstrap): bc_compiler core state management and helpers"
```

---

### Task 3: Expression Compilation — Literals, Variables, Arithmetic

**Files:**
- Modify: `bootstrap/bc_compiler.airl`
- Create: `bootstrap/bc_compiler_test.airl`

Add `bc-compile-expr` that handles literals, symbol loads, and direct arithmetic/comparison ops.

- [ ] **Step 1: Add `bc-compile-expr` for literals and variables**

Append to `bootstrap/bc_compiler.airl`:

```lisp
;; ── Core expression compiler ────────────────────────────

(defn bc-compile-expr
  :sig [(s : Any) (node : Any) (dst : Int) -> List]
  :intent "Compile AST expression to bytecode, return [new-state] or (Err msg)"
  :requires [(valid s) (valid node)]
  :ensures [(valid result)]
  :body (match node
    (ASTInt v _ _)
      (let (r : List (cs-add-const s v))
        (let (s2 : Any (at r 0))
          (let (idx : Int (at r 1))
            (Ok (cs-emit s2 (OP-LOAD-CONST) dst idx 0)))))

    (ASTFloat v _ _)
      (let (r : List (cs-add-const s v))
        (let (s2 : Any (at r 0))
          (let (idx : Int (at r 1))
            (Ok (cs-emit s2 (OP-LOAD-CONST) dst idx 0)))))

    (ASTStr v _ _)
      (let (r : List (cs-add-const s v))
        (let (s2 : Any (at r 0))
          (let (idx : Int (at r 1))
            (Ok (cs-emit s2 (OP-LOAD-CONST) dst idx 0)))))

    (ASTBool b _ _)
      (if b (Ok (cs-emit s (OP-LOAD-TRUE) dst 0 0))
            (Ok (cs-emit s (OP-LOAD-FALSE) dst 0 0)))

    (ASTNil _ _)
      (Ok (cs-emit s (OP-LOAD-NIL) dst 0 0))

    (ASTKeyword k _ _)
      (let (r : List (cs-add-const s (join [":" k] "")))
        (let (s2 : Any (at r 0))
          (let (idx : Int (at r 1))
            (Ok (cs-emit s2 (OP-LOAD-CONST) dst idx 0)))))

    (ASTSymbol name _ _)
      (let (reg : Any (cs-lookup-local s name))
        (if (valid reg)
          (Ok (cs-emit s (OP-MOVE) dst reg 0))
          ;; Not a local — emit as IRFuncRef for CallReg resolution
          (let (r : List (cs-add-const s (IRFuncRef name)))
            (let (s2 : Any (at r 0))
              (let (idx : Int (at r 1))
                (Ok (cs-emit s2 (OP-LOAD-CONST) dst idx 0)))))))

    ;; ... more patterns added in subsequent tasks
    _ (Err (+ "bc-compile-expr: unhandled node type: " (type-of node)))))
```

- [ ] **Step 2: Add function calls and direct arithmetic ops**

Add `ASTCall` handling to `bc-compile-expr` (before the wildcard):

```lisp
    (ASTCall callee args _ _)
      (match callee
        (ASTSymbol name _ _)
          ;; Check for direct arithmetic/comparison ops
          (let (direct-op : Any
            (if (= name "+") (OP-ADD)
            (if (= name "-") (OP-SUB)
            (if (= name "*") (OP-MUL)
            (if (= name "/") (OP-DIV)
            (if (= name "%") (OP-MOD)
            (if (= name "=") (OP-EQ)
            (if (= name "!=") (OP-NE)
            (if (= name "<") (OP-LT)
            (if (= name "<=") (OP-LE)
            (if (= name ">") (OP-GT)
            (if (= name ">=") (OP-GE)
            (if (= name "not") (OP-NOT)
              nil)))))))))))))
            (if (valid direct-op)
              ;; Direct opcode for binary/unary ops
              (if (= (length args) 2)
                (let (ra : List (cs-alloc-reg s))
                  (let (s2 : Any (at ra 0)) (let (a-reg : Int (at ra 1))
                    (match (bc-compile-expr s2 (at args 0) a-reg)
                      (Err e) (Err e)
                      (Ok s3)
                        (let (rb : List (cs-alloc-reg s3))
                          (let (s4 : Any (at rb 0)) (let (b-reg : Int (at rb 1))
                            (match (bc-compile-expr s4 (at args 1) b-reg)
                              (Err e) (Err e)
                              (Ok s5)
                                (Ok (cs-free-reg-to
                                  (cs-emit s5 direct-op dst a-reg b-reg)
                                  (+ dst 1)))))))))))
                ;; Unary (not)
                (if (= (length args) 1)
                  (let (ra : List (cs-alloc-reg s))
                    (let (s2 : Any (at ra 0)) (let (a-reg : Int (at ra 1))
                      (match (bc-compile-expr s2 (at args 0) a-reg)
                        (Err e) (Err e)
                        (Ok s3)
                          (Ok (cs-free-reg-to
                            (cs-emit s3 direct-op dst a-reg 0)
                            (+ dst 1)))))))
                  (Err "direct op: wrong arity")))
              ;; Named function call
              (bc-compile-named-call s name args dst)))
        ;; Computed callee (closure call)
        _ (bc-compile-call-expr s callee args dst))
```

- [ ] **Step 3: Add named call and builtin call helpers**

```lisp
(defn bc-compile-named-call
  :sig [(s : Any) (name : String) (args : List) (dst : Int) -> List]
  :intent "Compile a named function call"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body
    (let (argc : Int (length args))
      ;; Check if callee is a local (closure in register)
      (let (local-reg : Any (cs-lookup-local s name))
        (if (valid local-reg)
          ;; Closure call via CallReg
          (bc-compile-call-reg s local-reg args dst)
          ;; Named function call via Call opcode
          (let (rc : List (cs-add-const s name))
            (let (s2 : Any (at rc 0)) (let (name-idx : Int (at rc 1))
              (match (bc-compile-args-to-slots s2 args (+ dst 1))
                (Err e) (Err e)
                (Ok s3) (Ok (cs-emit s3 (OP-CALL) dst name-idx argc))))))))))

(defn bc-compile-call-expr
  :sig [(s : Any) (callee : Any) (args : List) (dst : Int) -> List]
  :intent "Compile a computed function call (closure)"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body
    (let (ra : List (cs-alloc-reg s))
      (let (s2 : Any (at ra 0)) (let (callee-reg : Int (at ra 1))
        (match (bc-compile-expr s2 callee callee-reg)
          (Err e) (Err e)
          (Ok s3) (bc-compile-call-reg s3 callee-reg args dst))))))

(defn bc-compile-call-reg
  :sig [(s : Any) (callee-reg : Int) (args : List) (dst : Int) -> List]
  :intent "Emit CallReg with args in slots"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body
    (let (argc : Int (length args))
      (match (bc-compile-args-to-slots s args (+ dst 1))
        (Err e) (Err e)
        (Ok s2) (Ok (cs-emit s2 (OP-CALL-REG) dst callee-reg argc)))))

(defn bc-compile-args-to-slots
  :sig [(s : Any) (args : List) (start-reg : Int) -> List]
  :intent "Compile args into consecutive registers starting at start-reg"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body
    (if (empty? args) (Ok s)
      (match (bc-compile-expr s (head args) start-reg)
        (Err e) (Err e)
        (Ok s2) (bc-compile-args-to-slots s2 (tail args) (+ start-reg 1)))))
```

- [ ] **Step 4: Write unit tests**

Create `bootstrap/bc_compiler_test.airl`:

```lisp
;; Test: compile integer literal and run via run-bytecode
(defn bc-test-int
  :sig [-> Any]
  :intent "Test compiling integer literal"
  :requires [true]
  :ensures [(valid result)]
  :body
    (let (s0 : Any (make-compiler-state))
      (let (r : List (cs-alloc-reg s0))
        (let (s1 : Any (at r 0)) (let (dst : Int (at r 1))
          (match (bc-compile-expr s1 (ASTInt 42 1 0) dst)
            (Err e) (do (print "FAIL bc-test-int:" e) false)
            (Ok s2)
              (let (s3 : Any (cs-emit s2 (OP-RETURN) 0 dst 0))
                (let (func : Any (BCFunc "__main__" 0 (cs-max-reg s3) 0
                                   (cs-consts s3) (cs-instrs s3)))
                  (let (result : Any (run-bytecode [func]))
                    (if (= result 42)
                      (do (print "PASS: bc-test-int") true)
                      (do (print "FAIL: bc-test-int expected 42 got" result) false)))))))))))

(bc-test-int)
```

- [ ] **Step 5: Run test**

```bash
cat bootstrap/lexer.airl bootstrap/parser.airl bootstrap/bc_compiler.airl bootstrap/bc_compiler_test.airl > /tmp/bc_test_full.airl
RUST_MIN_STACK=67108864 target/release/airl-driver run /tmp/bc_test_full.airl
```

Expected: `"PASS: bc-test-int"`

- [ ] **Step 6: Commit**

```bash
git add bootstrap/bc_compiler.airl bootstrap/bc_compiler_test.airl
git commit -m "feat(bootstrap): bc_compiler expression compilation — literals, variables, calls"
```

---

### Task 4: Control Flow — If, Do, Let

**Files:**
- Modify: `bootstrap/bc_compiler.airl`
- Modify: `bootstrap/bc_compiler_test.airl`

Add `ASTIf`, `ASTDo`, `ASTLet` to `bc-compile-expr`.

- [ ] **Step 1: Add If compilation**

Add before the wildcard in `bc-compile-expr`:

```lisp
    (ASTIf cond then-branch else-branch _ _)
      ;; Compile condition
      (let (ra : List (cs-alloc-reg s))
        (let (s2 : Any (at ra 0)) (let (cond-reg : Int (at ra 1))
          (match (bc-compile-expr s2 cond cond-reg)
            (Err e) (Err e)
            (Ok s3)
              ;; JumpIfFalse to else branch (patch later)
              (let (skip-idx : Int (cs-instr-count s3))
                (let (s4 : Any (cs-emit s3 (OP-JUMP-IF-FALSE) 0 cond-reg 0))
                  (let (s5 : Any (cs-free-reg-to s4 (+ dst 1)))
                    (match (bc-compile-expr s5 then-branch dst)
                      (Err e) (Err e)
                      (Ok s6)
                        ;; Jump over else branch
                        (let (end-jump-idx : Int (cs-instr-count s6))
                          (let (s7 : Any (cs-emit s6 (OP-JUMP) 0 0 0))
                            ;; Patch JumpIfFalse to here
                            (let (else-start : Int (cs-instr-count s7))
                              (let (s8 : Any (cs-patch-jump s7 skip-idx "b"
                                       (- else-start (+ skip-idx 1))))
                                (match (bc-compile-expr s8 else-branch dst)
                                  (Err e) (Err e)
                                  (Ok s9)
                                    ;; Patch end jump
                                    (let (end-pos : Int (cs-instr-count s9))
                                      (Ok (cs-patch-jump s9 end-jump-idx "a"
                                            (- end-pos (+ end-jump-idx 1)))))))))))))))))))
```

- [ ] **Step 2: Add Do compilation**

```lisp
    (ASTDo exprs _ _)
      (bc-compile-do s exprs dst)
```

With helper:

```lisp
(defn bc-compile-do
  :sig [(s : Any) (exprs : List) (dst : Int) -> List]
  :intent "Compile do block — last expression result goes to dst"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body
    (if (empty? exprs) (Ok (cs-emit s (OP-LOAD-NIL) dst 0 0))
      (if (= (length exprs) 1)
        (bc-compile-expr s (head exprs) dst)
        ;; Compile first to temp, rest continues
        (let (ra : List (cs-alloc-reg s))
          (let (s2 : Any (at ra 0)) (let (tmp : Int (at ra 1))
            (match (bc-compile-expr s2 (head exprs) tmp)
              (Err e) (Err e)
              (Ok s3) (bc-compile-do (cs-free-reg-to s3 (+ dst 1)) (tail exprs) dst))))))))
```

- [ ] **Step 3: Add Let compilation**

```lisp
    (ASTLet bindings body _ _)
      (match (bc-compile-let-bindings s bindings)
        (Err e) (Err e)
        (Ok s2)
          (match (bc-compile-expr s2 body dst)
            (Err e) (Err e)
            (Ok s3) (Ok (bc-unbind-let s3 bindings))))
```

With helpers:

```lisp
(defn bc-compile-let-bindings
  :sig [(s : Any) (bindings : List) -> List]
  :intent "Compile let bindings, adding each to locals"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body
    (if (empty? bindings) (Ok s)
      (match (head bindings)
        (ASTBinding name _ val-expr)
          (let (ra : List (cs-alloc-reg s))
            (let (s2 : Any (at ra 0)) (let (reg : Int (at ra 1))
              (match (bc-compile-expr s2 val-expr reg)
                (Err e) (Err e)
                (Ok s3) (bc-compile-let-bindings (cs-set-local s3 name reg)
                           (tail bindings))))))
        _ (Err "bc-compile-let-bindings: invalid binding"))))

(defn bc-unbind-let
  :sig [(s : Any) (bindings : List) -> Any]
  :intent "Remove let bindings from locals"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body
    (if (empty? bindings) s
      (match (head bindings)
        (ASTBinding name _ _) (bc-unbind-let (cs-remove-local s name) (tail bindings))
        _ s)))
```

- [ ] **Step 4: Add tests for if/do/let**

Append to `bootstrap/bc_compiler_test.airl`:

```lisp
;; Test: (if true 1 2) => 1
;; Test: (let (x 10) (+ x 5)) => 15
;; Test: (do 1 2 3) => 3
;; (Implementation: compile each, wrap in __main__, run-bytecode, assert)
```

- [ ] **Step 5: Run tests, commit**

---

### Task 5: Data Construction — Lists, Variants, Try

**Files:**
- Modify: `bootstrap/bc_compiler.airl`

- [ ] **Step 1: Add List, Variant, Try to bc-compile-expr**

```lisp
    (ASTList items _ _)
      ;; Compile items to consecutive regs, emit MakeList
      (let (ra : List (cs-alloc-regs s (length items)))
        (let (s2 : Any (at ra 0)) (let (start : Int (at ra 1))
          (match (bc-compile-items-to-regs s2 items start)
            (Err e) (Err e)
            (Ok s3) (Ok (cs-emit s3 (OP-MAKE-LIST) dst start (length items)))))))

    (ASTVariant name args _ _)
      (if (empty? args)
        ;; 0-arg variant
        (let (rc : List (cs-add-const s name))
          (let (s2 : Any (at rc 0)) (let (tag-idx : Int (at rc 1))
            (Ok (cs-emit s2 (OP-MAKE-VARIANT0) dst tag-idx 0)))))
        (if (= (length args) 1)
          ;; 1-arg variant
          (let (rc : List (cs-add-const s name))
            (let (s2 : Any (at rc 0)) (let (tag-idx : Int (at rc 1))
              (let (ra : List (cs-alloc-reg s2))
                (let (s3 : Any (at ra 0)) (let (inner-reg : Int (at ra 1))
                  (match (bc-compile-expr s3 (at args 0) inner-reg)
                    (Err e) (Err e)
                    (Ok s4) (Ok (cs-free-reg-to
                              (cs-emit s4 (OP-MAKE-VARIANT) dst tag-idx inner-reg)
                              (+ dst 1))))))))))
          ;; Multi-arg variant: pack into list, then wrap
          (let (ra : List (cs-alloc-regs s (length args)))
            (let (s2 : Any (at ra 0)) (let (start : Int (at ra 1))
              (match (bc-compile-items-to-regs s2 args start)
                (Err e) (Err e)
                (Ok s3)
                  (let (list-reg : Int (cs-next-reg s3))
                    (let (ra2 : List (cs-alloc-reg s3))
                      (let (s4 : Any (at ra2 0))
                        (let (s5 : Any (cs-emit s4 (OP-MAKE-LIST) list-reg start (length args)))
                          (let (rc : List (cs-add-const s5 name))
                            (let (s6 : Any (at rc 0)) (let (tag-idx : Int (at rc 1))
                              (Ok (cs-free-reg-to
                                (cs-emit s6 (OP-MAKE-VARIANT) dst tag-idx list-reg)
                                (+ dst 1))))))))))))))))

    (ASTTry expr _ _)
      (let (ra : List (cs-alloc-reg s))
        (let (s2 : Any (at ra 0)) (let (src-reg : Int (at ra 1))
          (match (bc-compile-expr s2 expr src-reg)
            (Err e) (Err e)
            (Ok s3)
              ;; TryUnwrap with error offset — for now, offset 0 (will crash on Err)
              ;; TODO: proper error handler offset
              (Ok (cs-free-reg-to
                (cs-emit s3 (OP-TRY-UNWRAP) dst src-reg 0)
                (+ dst 1)))))))
```

Helper for compiling items to consecutive registers:

```lisp
(defn bc-compile-items-to-regs
  :sig [(s : Any) (items : List) (reg : Int) -> List]
  :intent "Compile each item into consecutive register slots"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body
    (if (empty? items) (Ok s)
      (match (bc-compile-expr s (head items) reg)
        (Err e) (Err e)
        (Ok s2) (bc-compile-items-to-regs s2 (tail items) (+ reg 1)))))
```

- [ ] **Step 2: Test and commit**

---

### Task 6: Pattern Matching

**Files:**
- Modify: `bootstrap/bc_compiler.airl`

This is the most complex compilation target. Pattern matching requires MatchTag, JumpIfNoMatch, and multi-field variant destructuring with `at` calls.

- [ ] **Step 1: Add Match compilation to bc-compile-expr**

```lisp
    (ASTMatch scrutinee arms _ _)
      (let (ra : List (cs-alloc-reg s))
        (let (s2 : Any (at ra 0)) (let (scr-reg : Int (at ra 1))
          (match (bc-compile-expr s2 scrutinee scr-reg)
            (Err e) (Err e)
            (Ok s3) (bc-compile-match-arms s3 arms scr-reg dst [])))))
```

- [ ] **Step 2: Implement match arm compilation**

```lisp
(defn bc-compile-match-arms
  :sig [(s : Any) (arms : List) (scr-reg : Int) (dst : Int) (end-jumps : List) -> List]
  :intent "Compile match arms with pattern dispatch"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body
    (if (empty? arms)
      ;; Patch all end-jumps to here
      (Ok (bc-patch-end-jumps s end-jumps))
      (match (head arms)
        (ASTArm pat body)
          (match pat
            (PatWild _ _)
              ;; Wildcard always matches
              (let (s2 : Any (cs-emit s (OP-MATCH-WILD) dst scr-reg 0))
                (match (bc-compile-expr s2 body dst)
                  (Err e) (Err e)
                  (Ok s3) (Ok (bc-patch-end-jumps s3 end-jumps))))

            (PatBind name _ _)
              ;; Bind scrutinee to name
              (let (s2 : Any (cs-set-local s name scr-reg))
                (match (bc-compile-expr s2 body dst)
                  (Err e) (Err e)
                  (Ok s3) (Ok (bc-patch-end-jumps (cs-remove-local s3 name) end-jumps))))

            (PatLit value _ _)
              ;; Compare against literal
              (let (ra : List (cs-alloc-reg s))
                (let (s2 : Any (at ra 0)) (let (val-reg : Int (at ra 1))
                  (let (rc : List (cs-add-const s2 value))
                    (let (s3 : Any (at rc 0)) (let (val-idx : Int (at rc 1))
                      (let (s4 : Any (cs-emit s3 (OP-LOAD-CONST) val-reg val-idx 0))
                        (let (s5 : Any (cs-emit s4 (OP-EQ) val-reg scr-reg val-reg))
                          (let (skip-idx : Int (cs-instr-count s5))
                            (let (s6 : Any (cs-emit s5 (OP-JUMP-IF-FALSE) 0 val-reg 0))
                              (let (s7 : Any (cs-free-reg-to s6 (+ dst 1)))
                                (match (bc-compile-expr s7 body dst)
                                  (Err e) (Err e)
                                  (Ok s8)
                                    (let (ej-idx : Int (cs-instr-count s8))
                                      (let (s9 : Any (cs-emit s8 (OP-JUMP) 0 0 0))
                                        (let (here : Int (cs-instr-count s9))
                                          (let (s10 : Any (cs-patch-jump s9 skip-idx "b"
                                                    (- here (+ skip-idx 1))))
                                            (bc-compile-match-arms s10 (tail arms) scr-reg dst
                                              (append end-jumps ej-idx)))))))))))))))))

            (PatVariant tag sub-pats _ _)
              ;; Variant pattern match
              (let (rc : List (cs-add-const s tag))
                (let (s2 : Any (at rc 0)) (let (tag-idx : Int (at rc 1))
                  (let (ra : List (cs-alloc-reg s2))
                    (let (s3 : Any (at ra 0)) (let (inner-reg : Int (at ra 1))
                      (let (s4 : Any (cs-emit s3 (OP-MATCH-TAG) inner-reg scr-reg tag-idx))
                        (let (skip-idx : Int (cs-instr-count s4))
                          (let (s5 : Any (cs-emit s4 (OP-JUMP-IF-NO-MATCH) 0 0 0))
                            ;; Bind sub-patterns
                            (let (s6 : Any (bc-bind-sub-patterns s5 sub-pats inner-reg))
                              (match (bc-compile-expr s6 body dst)
                                (Err e) (Err e)
                                (Ok s7)
                                  (let (s8 : Any (bc-unbind-sub-patterns s7 sub-pats))
                                    (let (ej-idx : Int (cs-instr-count s8))
                                      (let (s9 : Any (cs-emit s8 (OP-JUMP) 0 0 0))
                                        (let (here : Int (cs-instr-count s9))
                                          (let (s10 : Any (cs-patch-jump s9 skip-idx "a"
                                                    (- here (+ skip-idx 1))))
                                            (bc-compile-match-arms s10 (tail arms) scr-reg dst
                                              (append end-jumps ej-idx)))))))))))))))))

            _ (Err "bc-compile-match-arms: unknown pattern type"))
        _ (Err "bc-compile-match-arms: invalid arm"))))
```

- [ ] **Step 3: Implement pattern binding helpers**

```lisp
(defn bc-bind-sub-patterns
  :sig [(s : Any) (pats : List) (inner-reg : Int) -> Any]
  :intent "Bind sub-patterns — single field uses inner directly, multi-field uses at(inner, i)"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body
    (if (= (length pats) 1)
      ;; Single sub-pattern: bind directly to inner value
      (bc-bind-single-pattern s (at pats 0) inner-reg)
      ;; Multi-field: inner is a list, extract by index
      (bc-bind-multi-field s pats inner-reg 0)))

(defn bc-bind-single-pattern
  :sig [(s : Any) (pat : Any) (reg : Int) -> Any]
  :intent "Bind a single pattern to a register"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body (match pat
    (PatBind name _ _) (cs-set-local s name reg)
    (PatWild _ _) s
    _ s))

(defn bc-bind-multi-field
  :sig [(s : Any) (pats : List) (inner-reg : Int) (idx : Int) -> Any]
  :intent "Bind multi-field variant patterns via at(inner, idx)"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body
    (if (empty? pats) s
      (let (pat : Any (head pats))
        (match pat
          (PatBind name _ _)
            ;; Emit: dst = at(inner, idx)
            (let (ra : List (cs-alloc-regs s 3))
              (let (s2 : Any (at ra 0)) (let (call-dst : Int (at ra 1))
                (let (rc : List (cs-add-const s2 "at"))
                  (let (s3 : Any (at rc 0)) (let (at-idx : Int (at rc 1))
                    (let (rc2 : List (cs-add-const s3 idx))
                      (let (s4 : Any (at rc2 0)) (let (idx-const : Int (at rc2 1))
                        (let (s5 : Any (cs-emit s4 (OP-MOVE) (+ call-dst 1) inner-reg 0))
                          (let (s6 : Any (cs-emit s5 (OP-LOAD-CONST) (+ call-dst 2) idx-const 0))
                            (let (s7 : Any (cs-emit s6 (OP-CALL-BUILTIN) call-dst at-idx 2))
                              (bc-bind-multi-field (cs-set-local s7 name call-dst)
                                (tail pats) inner-reg (+ idx 1)))))))))))))
          (PatWild _ _)
            (bc-bind-multi-field s (tail pats) inner-reg (+ idx 1))
          _ (bc-bind-multi-field s (tail pats) inner-reg (+ idx 1))))))

(defn bc-unbind-sub-patterns
  :sig [(s : Any) (pats : List) -> Any]
  :intent "Remove sub-pattern bindings from locals"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body
    (if (empty? pats) s
      (match (head pats)
        (PatBind name _ _) (bc-unbind-sub-patterns (cs-remove-local s name) (tail pats))
        _ (bc-unbind-sub-patterns s (tail pats)))))

(defn bc-patch-end-jumps
  :sig [(s : Any) (indices : List) -> Any]
  :intent "Patch all end-of-arm jumps to current position"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body
    (if (empty? indices) s
      (let (idx : Int (head indices))
        (let (here : Int (cs-instr-count s))
          (bc-patch-end-jumps
            (cs-patch-jump s idx "a" (- here (+ idx 1)))
            (tail indices))))))
```

- [ ] **Step 4: Test pattern matching, commit**

---

### Task 7: Lambda/Closure Compilation

**Files:**
- Modify: `bootstrap/bc_compiler.airl`

Closures require free variable analysis, creating a separate function, and emitting MakeClosure.

- [ ] **Step 1: Implement free variable analysis**

```lisp
(defn bc-free-vars
  :sig [(node : Any) (bound : Any) -> List]
  :intent "Find free variables in an expression (not in bound set)"
  :requires [(valid node)]
  :ensures [(valid result)]
  :body (match node
    (ASTSymbol name _ _)
      (if (map-has bound name) [] [name])
    (ASTInt _ _ _) []
    (ASTFloat _ _ _) []
    (ASTStr _ _ _) []
    (ASTBool _ _ _) []
    (ASTNil _ _) []
    (ASTKeyword _ _ _) []
    (ASTIf c t e _ _)
      (concat (bc-free-vars c bound)
        (concat (bc-free-vars t bound) (bc-free-vars e bound)))
    (ASTCall callee args _ _)
      (concat (bc-free-vars callee bound) (bc-free-vars-list args bound))
    (ASTDo exprs _ _)
      (bc-free-vars-list exprs bound)
    (ASTLet bindings body _ _)
      (let (bvars : List (bc-free-vars-bindings bindings bound))
        (let (new-bound : Any (bc-extend-bound bound bindings))
          (concat bvars (bc-free-vars body new-bound))))
    (ASTLambda params body _ _)
      (let (inner-bound : Any (fold (fn [b p] (map-set b p true)) bound params))
        (bc-free-vars body inner-bound))
    (ASTMatch scr arms _ _)
      (concat (bc-free-vars scr bound) (bc-free-vars-arms arms bound))
    (ASTList items _ _)
      (bc-free-vars-list items bound)
    (ASTVariant _ args _ _)
      (bc-free-vars-list args bound)
    (ASTTry expr _ _)
      (bc-free-vars expr bound)
    _ []))
```

(With helper functions `bc-free-vars-list`, `bc-free-vars-bindings`, `bc-extend-bound`, `bc-free-vars-arms`.)

- [ ] **Step 2: Implement Lambda compilation in bc-compile-expr**

```lisp
    (ASTLambda params body _ _)
      ;; 1. Find free variables captured from enclosing scope
      ;; 2. Compile lambda body as separate function (captures prepended to params)
      ;; 3. Emit MakeClosure copying captures to consecutive regs
      (let (param-bound : Any (fold (fn [b p] (map-set b p true)) (map-new) params))
        (let (free : List (bc-dedupe (bc-free-vars body param-bound)))
          (let (captured : List (filter (fn [v] (valid (cs-lookup-local s v))) free))
            ;; Generate unique lambda name
            (let (nr : List (cs-next-lambda-name s "user"))
              (let (s2 : Any (at nr 0)) (let (lambda-name : String (at nr 1))
                ;; All params = captures + lambda params
                (let (all-params : List (concat captured params))
                  ;; Compile lambda body as a new function
                  (let (func : Any (bc-compile-function lambda-name all-params body (length captured)))
                    (match func
                      (Err e) (Err e)
                      (Ok bc-func)
                        ;; Add to lambdas list
                        (let (s3 : Any (cs-add-lambda s2 bc-func))
                          ;; Emit MakeClosure
                          (let (rc : List (cs-add-const s3 lambda-name))
                            (let (s4 : Any (at rc 0)) (let (name-idx : Int (at rc 1))
                              ;; Copy captures to consecutive regs
                              (let (cap-start : Int (cs-next-reg s4))
                                (let (s5 : Any (bc-copy-captures s4 captured cap-start))
                                  (Ok (cs-emit s5 (OP-MAKE-CLOSURE) dst name-idx cap-start)))))))))))))))))
```

- [ ] **Step 3: Implement bc-compile-function**

```lisp
(defn bc-compile-function
  :sig [(name : String) (params : List) (body : Any) (capture-count : Int) -> List]
  :intent "Compile a function body into a BCFunc"
  :requires [(valid name) (valid params)]
  :ensures [(valid result)]
  :body
    (let (s0 : Any (make-compiler-state))
      ;; Bind params to registers 0..N-1
      (let (s1 : Any (bc-bind-params s0 params 0))
        (let (ra : List (cs-alloc-regs s1 (length params)))
          (let (s2 : Any (at ra 0))
            (let (rd : List (cs-alloc-reg s2))
              (let (s3 : Any (at rd 0)) (let (dst : Int (at rd 1))
                (match (bc-compile-expr s3 body dst)
                  (Err e) (Err e)
                  (Ok s4)
                    (let (s5 : Any (cs-emit s4 (OP-RETURN) 0 dst 0))
                      (Ok (BCFunc name (length params) (cs-max-reg s5) capture-count
                             (cs-consts s5) (cs-instrs s5)))))))))))))
```

- [ ] **Step 4: Test closures, commit**

---

### Task 8: Top-Level Compilation and Program Assembly

**Files:**
- Modify: `bootstrap/bc_compiler.airl`

- [ ] **Step 1: Implement top-level form compilation**

```lisp
(defn bc-compile-top-level
  :sig [(node : Any) -> List]
  :intent "Compile a top-level form (defn or expression)"
  :requires [(valid node)]
  :ensures [(valid result)]
  :body (match node
    (ASTDefn name sig _ _ _ body _ _)
      (match sig
        (ASTSig params ret-name)
          (let (param-names : List (map (fn [p] (match p (ASTParam n _) n _ "?")) params))
            (bc-compile-function name param-names body 0))
        _ (Err "bc-compile-top-level: defn requires sig"))
    (ASTDefType _ _ _ _ _) (Ok nil)  ;; deftype is metadata, skip
    _ (Err "unexpected top-level form")))

(defn bc-compile-program
  :sig [(ast-nodes : List) -> List]
  :intent "Compile all top-level AST nodes to a list of BCFunc values"
  :requires [(valid ast-nodes)]
  :ensures [(valid result)]
  :body
    ;; Separate defn's from expressions
    (let (defns : List (filter (fn [n] (match n (ASTDefn _ _ _ _ _ _ _ _) true _ false)) ast-nodes))
      (let (exprs : List (filter (fn [n] (match n (ASTDefn _ _ _ _ _ _ _ _) false (ASTDefType _ _ _ _ _) false _ true)) ast-nodes))
        ;; Compile each defn
        (match (bc-compile-defns defns)
          (Err e) (Err e)
          (Ok func-list)
            ;; Compile expressions as __main__
            (match (bc-compile-main-body exprs)
              (Err e) (Err e)
              (Ok main-result)
                ;; Collect: defn funcs + main func + all lambdas
                (let (main-func : Any (at main-result 0))
                  (let (lambdas : List (at main-result 1))
                    (Ok (concat func-list (concat lambdas [main-func]))))))))))
```

- [ ] **Step 2: Implement run-compiled-bytecode pipeline**

```lisp
(defn run-compiled-bc
  :sig [(source : String) -> List]
  :intent "Full pipeline: source -> lex -> parse -> compile-bytecode -> run-bytecode"
  :requires [(valid source)]
  :ensures [(valid result)]
  :body
    (match (lex source)
      (Err e) (Err e)
      (Ok tokens)
        (match (parse-sexpr-all tokens)
          (Err e) (Err e)
          (Ok sexprs)
            (match (parse-program sexprs)
              (Err e) (Err e)
              (Ok ast-nodes)
                (match (bc-compile-program ast-nodes)
                  (Err e) (Err e)
                  (Ok bc-funcs) (Ok (run-bytecode bc-funcs)))))))
```

- [ ] **Step 3: Test with simple programs, commit**

---

### Task 9: Equivalence Test

**Files:**
- Create: `bootstrap/bc_equivalence_test.airl`

- [ ] **Step 1: Create equivalence test comparing interpreter vs bytecode compiler**

Pattern: for each test case, run the source through the interpreter AND through `run-compiled-bc`, assert results match. Cover: literals, arithmetic, if, let, do, functions, recursion, pattern matching, closures, lists, variants.

- [ ] **Step 2: Run equivalence test**

```bash
cat bootstrap/lexer.airl bootstrap/parser.airl bootstrap/bc_compiler.airl bootstrap/bc_equivalence_test.airl > /tmp/bc_equiv_full.airl
RUST_MIN_STACK=67108864 target/release/airl-driver run --release /tmp/bc_equiv_full.airl
```

- [ ] **Step 3: Commit**

```bash
git add bootstrap/bc_equivalence_test.airl
git commit -m "test(bootstrap): bytecode compiler equivalence test — interpreter vs compiled bytecode"
```

---

### Task 10: Integration and Cleanup

- [ ] **Step 1: Run all existing tests to confirm no regressions**

```bash
source "$HOME/.cargo/env" && RUST_MIN_STACK=67108864 cargo test --workspace --exclude airl-mlir --features jit 2>&1 | grep "test result:"
```

- [ ] **Step 2: Run all bootstrap tests on --bytecode**

```bash
for test in bootstrap/lexer_test.airl bootstrap/parser_test.airl bootstrap/eval_test.airl bootstrap/compiler_test.airl bootstrap/types_test.airl bootstrap/integration_test.airl bootstrap/deftype_test.airl bootstrap/compiler_integration_test.airl bootstrap/equivalence_test.airl bootstrap/pipeline_test.airl; do
  echo -n "$(basename $test): "
  RUST_MIN_STACK=67108864 timeout 120 target/release/airl-driver run --bytecode "$test" 2>&1 | tail -1
done
```

- [ ] **Step 3: Update CLAUDE.md with new bytecode compiler status**

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "feat(bootstrap): complete bytecode-emitting bootstrap compiler (v0.2 Phase 3)"
```
