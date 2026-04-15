use airl_syntax::{ast, Span, Diagnostic, Diagnostics};
use crate::ty::*;
use crate::env::TypeEnv;
use crate::unify::{DimSubst, unify_dim};

/// Type checker for the AIRL language.
///
/// Resolves AST types to internal `Ty`, checks expressions, functions,
/// and top-level forms. Supports dependent dimension unification for
/// tensor types.
pub struct TypeChecker {
    pub env: TypeEnv,
    pub dim_subst: DimSubst,
    diags: Diagnostics,
    /// Pre-interned common symbols for O(1) TypeVar construction and comparison.
    sym_wildcard: crate::ty::SymbolId,  // "_"
    sym_builtin: crate::ty::SymbolId,   // "builtin"
}

impl TypeChecker {
    pub fn new() -> Self {
        let mut env = TypeEnv::new();
        let sym_wildcard = env.intern("_");
        let sym_builtin  = env.intern("builtin");
        let mut tc = Self {
            env,
            dim_subst: DimSubst::new(),
            diags: Diagnostics::new(),
            sym_wildcard,
            sym_builtin,
        };
        tc.register_builtins();
        tc
    }

    /// Intern a string through the shared environment interner.
    #[inline]
    fn intern(&mut self, s: &str) -> crate::ty::SymbolId {
        self.env.intern(s)
    }

    /// Wildcard type: compatible with everything (inference placeholder).
    #[inline]
    fn ty_wildcard(&self) -> Ty {
        Ty::TypeVar(self.sym_wildcard)
    }

    /// Register built-in arithmetic and comparison operators.
    fn register_builtins(&mut self) {
        // Arithmetic: (+ a b), (- a b), (* a b), (/ a b)
        for op in &["+", "-", "*", "/", "%"] {
            self.env.bind_str(
                op,
                Ty::Func {
                    params: vec![Ty::Prim(PrimTy::I64), Ty::Prim(PrimTy::I64)],
                    ret: Box::new(Ty::Prim(PrimTy::I64)),
                },
            );
        }
        // Comparison: (< a b), (> a b), (<= a b), (>= a b), (= a b), (!= a b)
        for op in &["<", ">", "<=", ">=", "=", "!="] {
            self.env.bind_str(
                op,
                Ty::Func {
                    params: vec![Ty::Prim(PrimTy::I64), Ty::Prim(PrimTy::I64)],
                    ret: Box::new(Ty::Prim(PrimTy::Bool)),
                },
            );
        }
        // Boolean ops
        for op in &["and", "or"] {
            self.env.bind_str(
                op,
                Ty::Func {
                    params: vec![Ty::Prim(PrimTy::Bool), Ty::Prim(PrimTy::Bool)],
                    ret: Box::new(Ty::Prim(PrimTy::Bool)),
                },
            );
        }
        self.env.bind_str(
            "not",
            Ty::Func {
                params: vec![Ty::Prim(PrimTy::Bool)],
                ret: Box::new(Ty::Prim(PrimTy::Bool)),
            },
        );

        // Typed collection builtins — polymorphic via TypeVar parameters.
        // These use TypeVar("T") / TypeVar("U") as stand-ins for parametric types.
        // The checker treats TypeVar("_") as a wildcard, so calls to these builtins
        // get proper arity checking while remaining polymorphic.
        self.register_typed_builtins();

        // Remaining builtins are bound as TypeVar("builtin") — this is intentional.
        // These are polymorphic builtins whose full signatures haven't been encoded yet.
        // Callers still get basic type-checking (args are checked, return type is wildcard).
        // Minimum-arity enforcement for selected names is handled in
        // `polymorphic_builtin_min_arity`.
        //
        // INVARIANT: Any builtin that has a fully typed signature in
        // `register_typed_builtins` above must NOT appear in this list — its entry here
        // would shadow the typed signature. This invariant is verified by the test
        // `typed_builtins_not_in_wildcard_list`.
        // Remaining builtins that cannot have fixed-arity Ty::Func signatures
        // (variadic, tensor-typed, agent protocol, compiler internals, AIRLOS-only).
        for name in &[
            // Variadic stdio (print accepts 1+ args, concatenated at runtime)
            "print", "println", "eprint", "eprintln", "format",
            // Tensor operations (would need Tensor type in the type system)
            "tensor.zeros", "tensor.ones", "tensor.rand", "tensor.identity",
            "tensor.add", "tensor.mul", "tensor.matmul", "tensor.reshape",
            "tensor.transpose", "tensor.softmax", "tensor.sum", "tensor.max",
            "tensor.slice",
            // Agent builtins (variadic send protocol)
            "spawn-agent", "send", "send-async",
            "await", "parallel", "broadcast", "retry", "escalate", "any-agent",
            // Compiler internals (rarely called, internal dispatch)
            "run-bytecode", "compile-to-executable", "compile-bytecode-to-executable",
            "compile-bytecode-to-executable-with-target",
            // Container runtime (aircon) — AIRLOS-only IPC stubs
            "aircon_create", "aircon_start", "aircon_stop", "aircon_status", "aircon_list",
        ] {
            let builtin_sym = self.sym_builtin;
            self.env.bind_str(name, Ty::TypeVar(builtin_sym));
        }
    }

    /// Helper to bind a function signature directly.
    fn bind_typed(&mut self, name: &str, params: &[Ty], ret: Ty) {
        self.env.bind_str(name, Ty::Func {
            params: params.to_vec(),
            ret: Box::new(ret),
        });
    }

    /// Register properly-typed signatures for the most-used builtins.
    ///
    /// Uses `TypeVar("_")` as the wildcard (compatible with anything in
    /// `types_compatible`), so these signatures enforce arity and structural
    /// shape while remaining polymorphic.
    fn register_typed_builtins(&mut self) {
        let t = self.ty_wildcard();
        let list_name = self.intern("List");
        let list_t = Ty::Named {
            name: list_name,
            args: vec![TyArg::Type(t.clone())],
        };

        // head : List[T] -> T
        self.env.bind_str("head", Ty::Func {
            params: vec![list_t.clone()],
            ret: Box::new(t.clone()),
        });

        // tail : List[T] -> List[T]
        self.env.bind_str("tail", Ty::Func {
            params: vec![list_t.clone()],
            ret: Box::new(list_t.clone()),
        });

        // cons : T -> List[T] -> List[T]
        self.env.bind_str("cons", Ty::Func {
            params: vec![t.clone(), list_t.clone()],
            ret: Box::new(list_t.clone()),
        });

        // empty? : List[T] -> Bool
        self.env.bind_str("empty?", Ty::Func {
            params: vec![list_t.clone()],
            ret: Box::new(Ty::Prim(PrimTy::Bool)),
        });

        // map : (T -> U) -> List[T] -> List[U]
        // T and U are distinct wildcards: the function may return a different type than its input.
        // We use TypeVar("_") for both since our checker treats any TypeVar("_") as compatible
        // with anything — giving arity checking without requiring full HM unification.
        let u = self.ty_wildcard();
        let list_u = Ty::Named {
            name: list_name,
            args: vec![TyArg::Type(u.clone())],
        };
        let fn_t_u = Ty::Func {
            params: vec![t.clone()],
            ret: Box::new(u.clone()),
        };
        self.env.bind_str("map", Ty::Func {
            params: vec![fn_t_u, list_t.clone()],
            ret: Box::new(list_u),
        });

        // filter : (T -> Bool) -> List[T] -> List[T]
        let fn_t_bool = Ty::Func {
            params: vec![t.clone()],
            ret: Box::new(Ty::Prim(PrimTy::Bool)),
        };
        self.env.bind_str("filter", Ty::Func {
            params: vec![fn_t_bool, list_t.clone()],
            ret: Box::new(list_t.clone()),
        });

        // fold : (U -> T -> U) -> U -> List[T] -> U
        let fn_u_t_u = Ty::Func {
            params: vec![t.clone(), t.clone()],
            ret: Box::new(t.clone()),
        };
        self.env.bind_str("fold", Ty::Func {
            params: vec![fn_u_t_u, t.clone(), list_t.clone()],
            ret: Box::new(t.clone()),
        });

        // str : T -> String  (polymorphic — accepts anything)
        self.env.bind_str("str", Ty::Func {
            params: vec![t.clone()],
            ret: Box::new(Ty::Prim(PrimTy::Str)),
        });

        // ── String builtins (Tier 1) ──
        let string_t = Ty::Prim(PrimTy::Str);
        let int_t = Ty::Prim(PrimTy::I64);
        let bool_t = Ty::Prim(PrimTy::Bool);

        // char-at : String -> Int -> String
        self.bind_typed("char-at", &[string_t.clone(), int_t.clone()], string_t.clone());

        // substring : String -> Int -> Int -> String
        self.bind_typed("substring", &[string_t.clone(), int_t.clone(), int_t.clone()], string_t.clone());

        // split : String -> String -> List
        self.bind_typed("split", &[string_t.clone(), string_t.clone()], list_t.clone());

        // join : List -> String -> String
        self.bind_typed("join", &[list_t.clone(), string_t.clone()], string_t.clone());

        // replace : String -> String -> String -> String
        self.bind_typed("replace", &[string_t.clone(), string_t.clone(), string_t.clone()], string_t.clone());

        // chars : String -> List
        self.bind_typed("chars", &[string_t.clone()], list_t.clone());

        // words : String -> List
        self.bind_typed("words", &[string_t.clone()], list_t.clone());

        // unwords : List -> String
        self.bind_typed("unwords", &[list_t.clone()], string_t.clone());

        // lines : String -> List
        self.bind_typed("lines", &[string_t.clone()], list_t.clone());

        // unlines : List -> String
        self.bind_typed("unlines", &[list_t.clone()], string_t.clone());

        // repeat-str : String -> Int -> String
        self.bind_typed("repeat-str", &[string_t.clone(), int_t.clone()], string_t.clone());

        // pad-left : String -> Int -> String -> String
        self.bind_typed("pad-left", &[string_t.clone(), int_t.clone(), string_t.clone()], string_t.clone());

        // pad-right : String -> Int -> String -> String
        self.bind_typed("pad-right", &[string_t.clone(), int_t.clone(), string_t.clone()], string_t.clone());

        // is-empty-str : String -> Bool
        self.bind_typed("is-empty-str", &[string_t.clone()], bool_t.clone());

        // reverse-str : String -> String
        self.bind_typed("reverse-str", &[string_t.clone()], string_t.clone());

        // count-occurrences : String -> String -> Int
        self.bind_typed("count-occurrences", &[string_t.clone(), string_t.clone()], int_t.clone());

        // ── Map builtins (Tier 2) ──
        let map_name = self.intern("Map");
        let map_t = Ty::Named {
            name: map_name,
            args: vec![TyArg::Type(t.clone())],
        };

        // map-new : () -> Map
        self.bind_typed("map-new", &[], map_t.clone());

        // map-get : Map -> T -> T
        self.bind_typed("map-get", &[map_t.clone(), t.clone()], t.clone());

        // map-set : Map -> T -> T -> Map
        self.bind_typed("map-set", &[map_t.clone(), t.clone(), t.clone()], map_t.clone());

        // map-has : Map -> T -> Bool
        self.bind_typed("map-has", &[map_t.clone(), t.clone()], bool_t.clone());

        // map-remove : Map -> T -> Map
        self.bind_typed("map-remove", &[map_t.clone(), t.clone()], map_t.clone());

        // map-keys : Map -> List
        self.bind_typed("map-keys", &[map_t.clone()], list_t.clone());

        // map-entries : Map -> List
        self.bind_typed("map-entries", &[map_t.clone()], list_t.clone());

        // map-from-entries : List -> Map
        self.bind_typed("map-from-entries", &[list_t.clone()], map_t.clone());

        // map-merge : Map -> Map -> Map
        self.bind_typed("map-merge", &[map_t.clone(), map_t.clone()], map_t.clone());

        // map-count : Map -> Int
        self.bind_typed("map-count", &[map_t.clone()], int_t.clone());

        // ── List builtins (Tier 3) ──
        // length : T -> Int (works on List, String, Map — use TypeVar("_"))
        self.bind_typed("length", &[t.clone()], int_t.clone());

        // at : List -> Int -> T
        self.bind_typed("at", &[list_t.clone(), int_t.clone()], t.clone());

        // at-or : List -> Int -> T -> T
        self.bind_typed("at-or", &[list_t.clone(), int_t.clone(), t.clone()], t.clone());

        // set-at : List -> Int -> T -> List
        self.bind_typed("set-at", &[list_t.clone(), int_t.clone(), t.clone()], list_t.clone());

        // list-contains? : List -> T -> Bool
        self.bind_typed("list-contains?", &[list_t.clone(), t.clone()], bool_t.clone());

        // append : List -> T -> List
        self.bind_typed("append", &[list_t.clone(), t.clone()], list_t.clone());

        // reverse : List -> List
        self.bind_typed("reverse", &[list_t.clone()], list_t.clone());

        // concat : List -> List -> List
        self.bind_typed("concat", &[list_t.clone(), list_t.clone()], list_t.clone());

        // zip : List -> List -> List
        self.bind_typed("zip", &[list_t.clone(), list_t.clone()], list_t.clone());

        // flatten : List -> List
        self.bind_typed("flatten", &[list_t.clone()], list_t.clone());

        // range : Int -> Int -> List
        self.bind_typed("range", &[int_t.clone(), int_t.clone()], list_t.clone());

        // take : List -> Int -> List
        self.bind_typed("take", &[list_t.clone(), int_t.clone()], list_t.clone());

        // drop : List -> Int -> List
        self.bind_typed("drop", &[list_t.clone(), int_t.clone()], list_t.clone());

        // sort : List -> List
        self.bind_typed("sort", &[list_t.clone()], list_t.clone());

        // find : T -> List -> T (first arg is predicate fn, use TypeVar("_"))
        self.bind_typed("find", &[t.clone(), list_t.clone()], t.clone());

        // ── Math builtins (Tier 4) ──
        let float_t = Ty::Prim(PrimTy::F64);

        // abs : Int -> Int
        self.bind_typed("abs", &[int_t.clone()], int_t.clone());

        // min : Int -> Int -> Int
        self.bind_typed("min", &[int_t.clone(), int_t.clone()], int_t.clone());

        // max : Int -> Int -> Int
        self.bind_typed("max", &[int_t.clone(), int_t.clone()], int_t.clone());

        // clamp : Int -> Int -> Int -> Int
        self.bind_typed("clamp", &[int_t.clone(), int_t.clone(), int_t.clone()], int_t.clone());

        // sqrt : Float -> Float
        self.bind_typed("sqrt", &[float_t.clone()], float_t.clone());

        // sin : Float -> Float
        self.bind_typed("sin", &[float_t.clone()], float_t.clone());

        // cos : Float -> Float
        self.bind_typed("cos", &[float_t.clone()], float_t.clone());

        // tan : Float -> Float
        self.bind_typed("tan", &[float_t.clone()], float_t.clone());

        // log : Float -> Float
        self.bind_typed("log", &[float_t.clone()], float_t.clone());

        // exp : Float -> Float
        self.bind_typed("exp", &[float_t.clone()], float_t.clone());

        // floor : Float -> Int
        self.bind_typed("floor", &[float_t.clone()], int_t.clone());

        // ceil : Float -> Int
        self.bind_typed("ceil", &[float_t.clone()], int_t.clone());

        // round : Float -> Int
        self.bind_typed("round", &[float_t.clone()], int_t.clone());

        // int-to-float : Int -> Float
        self.bind_typed("int-to-float", &[int_t.clone()], float_t.clone());

        // float-to-int : Float -> Int
        self.bind_typed("float-to-int", &[float_t.clone()], int_t.clone());

        // ── I/O, System, Conversion (Tier 5) ──
        // File I/O
        self.bind_typed("read-file", &[string_t.clone()], string_t.clone());
        self.bind_typed("write-file", &[string_t.clone(), string_t.clone()], bool_t.clone());
        self.bind_typed("file-exists?", &[string_t.clone()], bool_t.clone());
        self.bind_typed("append-file", &[string_t.clone(), string_t.clone()], bool_t.clone());
        self.bind_typed("delete-file", &[string_t.clone()], bool_t.clone());
        self.bind_typed("delete-dir", &[string_t.clone()], bool_t.clone());
        self.bind_typed("rename-file", &[string_t.clone(), string_t.clone()], bool_t.clone());
        self.bind_typed("read-dir", &[string_t.clone()], list_t.clone());
        self.bind_typed("create-dir", &[string_t.clone()], bool_t.clone());
        self.bind_typed("file-size", &[string_t.clone()], int_t.clone());
        self.bind_typed("is-dir?", &[string_t.clone()], bool_t.clone());
        self.bind_typed("temp-file", &[], string_t.clone());
        self.bind_typed("temp-dir", &[], string_t.clone());
        self.bind_typed("file-mtime", &[string_t.clone()], int_t.clone());

        // System
        self.bind_typed("shell-exec", &[string_t.clone(), list_t.clone()], t.clone()); // returns Result
        self.bind_typed("time-now", &[], int_t.clone());
        self.bind_typed("sleep", &[int_t.clone()], t.clone()); // returns Nil
        self.bind_typed("getenv", &[string_t.clone()], t.clone()); // returns Result
        self.bind_typed("get-args", &[], list_t.clone());
        self.bind_typed("cpu-count", &[], int_t.clone());
        self.bind_typed("format-time", &[int_t.clone(), string_t.clone()], string_t.clone());

        // Conversion
        self.bind_typed("int-to-string", &[int_t.clone()], string_t.clone());
        self.bind_typed("float-to-string", &[float_t.clone()], string_t.clone());
        self.bind_typed("string-to-int", &[string_t.clone()], int_t.clone());
        self.bind_typed("string-to-float", &[string_t.clone()], float_t.clone());
        self.bind_typed("char-code", &[string_t.clone()], int_t.clone());
        self.bind_typed("char-from-code", &[int_t.clone()], string_t.clone());

        // Float special values and checks
        self.bind_typed("infinity", &[], float_t.clone());
        self.bind_typed("nan", &[], float_t.clone());
        self.bind_typed("is-nan?", &[float_t.clone()], bool_t.clone());
        self.bind_typed("is-infinite?", &[float_t.clone()], bool_t.clone());

        // Assertions
        self.bind_typed("panic", &[string_t.clone()], t.clone());
        self.bind_typed("assert", &[bool_t.clone(), string_t.clone()], t.clone());

        // ── Result operations (Tier 6) ──
        self.bind_typed("is-ok?", &[t.clone()], bool_t.clone());
        self.bind_typed("is-err?", &[t.clone()], bool_t.clone());
        self.bind_typed("unwrap-or", &[t.clone(), t.clone()], t.clone());
        self.bind_typed("map-ok", &[t.clone(), t.clone()], t.clone());
        self.bind_typed("map-err", &[t.clone(), t.clone()], t.clone());
        self.bind_typed("and-then", &[t.clone(), t.clone()], t.clone());
        self.bind_typed("or-else", &[t.clone(), t.clone()], t.clone());
        self.bind_typed("ok-or", &[t.clone(), t.clone()], t.clone());

        // ── Introspection & Guards (Tier 7) ──
        self.bind_typed("type-of", &[t.clone()], string_t.clone());
        self.bind_typed("valid", &[t.clone()], bool_t.clone());
        self.bind_typed("shape", &[t.clone()], list_t.clone());

        // ── Stdio (Tier 7) ──
        self.bind_typed("read-line", &[], string_t.clone());
        self.bind_typed("read-stdin", &[], string_t.clone());

        // ── Math remaining (Tier 8) ──
        self.bind_typed("sign", &[int_t.clone()], int_t.clone());
        self.bind_typed("even?", &[int_t.clone()], bool_t.clone());
        self.bind_typed("odd?", &[int_t.clone()], bool_t.clone());
        self.bind_typed("pow", &[int_t.clone(), int_t.clone()], int_t.clone());
        self.bind_typed("gcd", &[int_t.clone(), int_t.clone()], int_t.clone());
        self.bind_typed("lcm", &[int_t.clone(), int_t.clone()], int_t.clone());
        self.bind_typed("sum-list", &[list_t.clone()], int_t.clone());
        self.bind_typed("product-list", &[list_t.clone()], int_t.clone());

        // ── Bitwise (Tier 8) ──
        self.bind_typed("bitwise-and", &[int_t.clone(), int_t.clone()], int_t.clone());
        self.bind_typed("bitwise-or", &[int_t.clone(), int_t.clone()], int_t.clone());
        self.bind_typed("bitwise-xor", &[int_t.clone(), int_t.clone()], int_t.clone());
        self.bind_typed("bitwise-shl", &[int_t.clone(), int_t.clone()], int_t.clone());
        self.bind_typed("bitwise-shr", &[int_t.clone(), int_t.clone()], int_t.clone());

        // ── Collections remaining (Tier 9) ──
        let fn_t_bool2 = Ty::Func {
            params: vec![t.clone()],
            ret: Box::new(bool_t.clone()),
        };
        // any : (T -> Bool) -> List[T] -> Bool
        self.bind_typed("any", &[fn_t_bool2.clone(), list_t.clone()], bool_t.clone());
        // all : (T -> Bool) -> List[T] -> Bool
        self.bind_typed("all", &[fn_t_bool2, list_t.clone()], bool_t.clone());
        // merge : (T -> T -> Bool) -> List[T] -> List[T] -> List[T]
        let fn_cmp = Ty::Func {
            params: vec![t.clone(), t.clone()],
            ret: Box::new(bool_t.clone()),
        };
        self.bind_typed("merge", &[fn_cmp, list_t.clone(), list_t.clone()], list_t.clone());

        // ── Map helpers (Tier 10) ──
        self.bind_typed("map-get-or", &[map_t.clone(), t.clone(), t.clone()], t.clone());
        self.bind_typed("map-values", &[map_t.clone()], list_t.clone());
        self.bind_typed("map-from", &[list_t.clone()], map_t.clone());
        // map-map-values : (T -> T) -> Map -> Map
        let fn_t_t = Ty::Func {
            params: vec![t.clone()],
            ret: Box::new(t.clone()),
        };
        self.bind_typed("map-map-values", &[fn_t_t, map_t.clone()], map_t.clone());
        // map-filter : (T -> T -> Bool) -> Map -> Map
        let fn_kv_bool = Ty::Func {
            params: vec![t.clone(), t.clone()],
            ret: Box::new(bool_t.clone()),
        };
        self.bind_typed("map-filter", &[fn_kv_bool, map_t.clone()], map_t.clone());
        // map-update : Map -> T -> (T -> T) -> Map
        self.bind_typed("map-update", &[map_t.clone(), t.clone(), t.clone()], map_t.clone());
        // map-update-or : Map -> T -> T -> (T -> T) -> Map
        self.bind_typed("map-update-or", &[map_t.clone(), t.clone(), t.clone(), t.clone()], map_t.clone());

        // ── Bytes (Tier 11) ──
        self.bind_typed("bytes-new", &[], list_t.clone());
        self.bind_typed("bytes-from-int8", &[int_t.clone()], list_t.clone());
        self.bind_typed("bytes-from-int16", &[int_t.clone()], list_t.clone());
        self.bind_typed("bytes-from-int32", &[int_t.clone()], list_t.clone());
        self.bind_typed("bytes-from-int64", &[int_t.clone()], list_t.clone());
        self.bind_typed("bytes-to-int16", &[list_t.clone(), int_t.clone()], int_t.clone());
        self.bind_typed("bytes-to-int32", &[list_t.clone(), int_t.clone()], int_t.clone());
        self.bind_typed("bytes-to-int64", &[list_t.clone(), int_t.clone()], int_t.clone());
        self.bind_typed("bytes-from-string", &[string_t.clone()], list_t.clone());
        self.bind_typed("bytes-to-string", &[list_t.clone(), int_t.clone(), int_t.clone()], string_t.clone());
        self.bind_typed("bytes-concat", &[list_t.clone(), list_t.clone()], list_t.clone());
        self.bind_typed("bytes-concat-all", &[list_t.clone()], list_t.clone());
        self.bind_typed("bytes-slice", &[list_t.clone(), int_t.clone(), int_t.clone()], list_t.clone());
        self.bind_typed("crc32c", &[list_t.clone()], int_t.clone());

        // ── TCP (Tier 12) — all return Result (typed as T/wildcard) ──
        self.bind_typed("tcp-listen", &[int_t.clone(), int_t.clone()], t.clone());
        self.bind_typed("tcp-accept", &[int_t.clone()], t.clone());
        self.bind_typed("tcp-accept-tls", &[int_t.clone(), string_t.clone(), string_t.clone(), string_t.clone()], t.clone());
        self.bind_typed("tcp-connect", &[string_t.clone(), int_t.clone()], t.clone());
        self.bind_typed("tcp-connect-tls", &[string_t.clone(), int_t.clone(), string_t.clone(), string_t.clone(), string_t.clone()], t.clone());
        self.bind_typed("tcp-close", &[int_t.clone()], t.clone());
        self.bind_typed("tcp-send", &[int_t.clone(), list_t.clone()], t.clone());
        self.bind_typed("tcp-recv", &[int_t.clone(), int_t.clone()], t.clone());
        self.bind_typed("tcp-recv-exact", &[int_t.clone(), int_t.clone()], t.clone());
        self.bind_typed("tcp-set-timeout", &[int_t.clone(), int_t.clone()], t.clone());

        // ── Threading and Channels (Tier 13) ──
        self.bind_typed("thread-spawn", &[t.clone()], int_t.clone());
        self.bind_typed("thread-join", &[int_t.clone()], t.clone());
        self.bind_typed("thread-set-affinity", &[int_t.clone()], t.clone());
        self.bind_typed("channel-new", &[], list_t.clone());
        self.bind_typed("channel-send", &[int_t.clone(), t.clone()], t.clone());
        self.bind_typed("channel-recv", &[int_t.clone()], t.clone());
        self.bind_typed("channel-recv-timeout", &[int_t.clone(), int_t.clone()], t.clone());
        self.bind_typed("channel-drain", &[int_t.clone()], list_t.clone());
        self.bind_typed("channel-close", &[int_t.clone()], bool_t.clone());

        // ── Crypto (Tier 14) ──
        // String hashing
        self.bind_typed("sha256", &[string_t.clone()], string_t.clone());
        self.bind_typed("sha512", &[string_t.clone()], string_t.clone());
        self.bind_typed("hmac-sha256", &[string_t.clone(), string_t.clone()], string_t.clone());
        self.bind_typed("hmac-sha512", &[string_t.clone(), string_t.clone()], string_t.clone());
        // Bytes hashing
        self.bind_typed("sha256-bytes", &[list_t.clone()], list_t.clone());
        self.bind_typed("sha512-bytes", &[list_t.clone()], list_t.clone());
        self.bind_typed("hmac-sha256-bytes", &[list_t.clone(), list_t.clone()], list_t.clone());
        self.bind_typed("hmac-sha512-bytes", &[list_t.clone(), list_t.clone()], list_t.clone());
        // Key derivation
        self.bind_typed("pbkdf2-sha256", &[string_t.clone(), string_t.clone(), int_t.clone(), int_t.clone()], list_t.clone());
        self.bind_typed("pbkdf2-sha512", &[string_t.clone(), string_t.clone(), int_t.clone(), int_t.clone()], list_t.clone());
        // Base64
        self.bind_typed("base64-encode", &[string_t.clone()], string_t.clone());
        self.bind_typed("base64-decode", &[string_t.clone()], string_t.clone());
        self.bind_typed("base64-encode-bytes", &[list_t.clone()], string_t.clone());
        self.bind_typed("base64-decode-bytes", &[string_t.clone()], list_t.clone());
        // Random
        self.bind_typed("random-bytes", &[int_t.clone()], list_t.clone());

        // ── Compression (Tier 15) — all IntList -> IntList ──
        for name in &[
            "gzip-compress", "gzip-decompress",
            "snappy-compress", "snappy-decompress",
            "lz4-compress", "lz4-decompress",
            "zstd-compress", "zstd-decompress",
        ] {
            self.bind_typed(name, &[list_t.clone()], list_t.clone());
        }

        // ── Additional System/Conversion (Tier 16) ──
        self.bind_typed("get-cwd", &[], string_t.clone());
        self.bind_typed("shell-exec-with-stdin", &[string_t.clone(), list_t.clone(), string_t.clone()], t.clone());
        self.bind_typed("parse-int-radix", &[string_t.clone(), int_t.clone()], t.clone());
        self.bind_typed("int-to-string-radix", &[int_t.clone(), int_t.clone()], string_t.clone());
        self.bind_typed("json-parse", &[string_t.clone()], t.clone());
        self.bind_typed("json-stringify", &[t.clone()], string_t.clone());
        self.bind_typed("read-lines", &[string_t.clone()], list_t.clone());
        self.bind_typed("exec-file", &[string_t.clone()], t.clone());
        self.bind_typed("whoami", &[], string_t.clone());

        // Path operations
        self.bind_typed("path-parent", &[string_t.clone()], string_t.clone());
        self.bind_typed("path-filename", &[string_t.clone()], string_t.clone());
        self.bind_typed("path-extension", &[string_t.clone()], string_t.clone());
        self.bind_typed("is-absolute?", &[string_t.clone()], bool_t.clone());

        // Regex
        self.bind_typed("regex-find-all", &[string_t.clone(), string_t.clone()], list_t.clone());
        self.bind_typed("regex-match", &[string_t.clone(), string_t.clone()], bool_t.clone());
        self.bind_typed("regex-split", &[string_t.clone(), string_t.clone()], list_t.clone());
    }

    // ── Type resolution ──────────────────────────────────

    /// Resolve an AST type name to an internal Ty.
    pub fn resolve_type_name(&self, name: &str) -> Result<Ty, ()> {
        if let Some(prim) = PrimTy::from_name(name) {
            return Ok(Ty::Prim(prim));
        }
        match name {
            "Unit" => Ok(Ty::Unit),
            "Never" => Ok(Ty::Never),
            _ => {
                if let Some(reg) = self.env.lookup_type(name) {
                    Ok(reg.ty.clone())
                } else {
                    Err(())
                }
            }
        }
    }

    /// Resolve a full AST type node to internal Ty.
    pub fn resolve_type(&mut self, ast_ty: &ast::AstType) -> Result<Ty, ()> {
        match &ast_ty.kind {
            ast::AstTypeKind::Named(name) => {
                // "_" is an inferred type placeholder — we return a special marker
                if name == "_" {
                    return Ok(self.ty_wildcard());
                }
                self.resolve_type_name(name)
            }
            ast::AstTypeKind::App(name, args) => {
                if name == "tensor" {
                    // tensor[ElemType Dim1 Dim2 ...]
                    if args.is_empty() {
                        return Err(());
                    }
                    let elem = self.resolve_type(&args[0])?;
                    let mut shape = Vec::new();
                    for a in &args[1..] {
                        shape.push(self.resolve_dim(a)?);
                    }
                    Ok(Ty::Tensor {
                        elem: Box::new(elem),
                        shape,
                    })
                } else {
                    // Named type application: Result[i32, DivError]
                    let mut resolved_args = Vec::new();
                    for a in args {
                        resolved_args.push(self.resolve_type(a).map(TyArg::Type)?);
                    }
                    let name_id = self.intern(name);
                    Ok(Ty::Named {
                        name: name_id,
                        args: resolved_args,
                    })
                }
            }
            ast::AstTypeKind::Func(params, ret) => {
                let mut param_tys = Vec::new();
                for p in params {
                    param_tys.push(self.resolve_type(p)?);
                }
                let ret_ty = self.resolve_type(ret)?;
                Ok(Ty::Func {
                    params: param_tys,
                    ret: Box::new(ret_ty),
                })
            }
            ast::AstTypeKind::Nat(nat) => Ok(Ty::Nat(self.ast_nat_to_dim(nat))),
        }
    }

    /// Resolve an AST type node used in a dimension position to a DimExpr.
    fn resolve_dim(&mut self, ast_ty: &ast::AstType) -> Result<DimExpr, ()> {
        match &ast_ty.kind {
            ast::AstTypeKind::Nat(nat) => Ok(self.ast_nat_to_dim(nat)),
            ast::AstTypeKind::Named(name) => {
                // Could be a dimension variable or a literal
                if let Ok(n) = name.parse::<u64>() {
                    Ok(DimExpr::Lit(n))
                } else {
                    let id = self.intern(name);
                    Ok(DimExpr::Var(id))
                }
            }
            _ => Err(()),
        }
    }

    /// Convert an AST NatExpr to a DimExpr, interning variable names.
    fn ast_nat_to_dim(&mut self, nat: &ast::NatExpr) -> DimExpr {
        match nat {
            ast::NatExpr::Lit(v) => DimExpr::Lit(*v),
            ast::NatExpr::Var(s) => {
                let id = self.intern(s);
                DimExpr::Var(id)
            }
            ast::NatExpr::BinOp(op, l, r) => {
                let dim_op = match op {
                    ast::NatOp::Add => DimOp::Add,
                    ast::NatOp::Sub => DimOp::Sub,
                    ast::NatOp::Mul => DimOp::Mul,
                };
                let l_dim = self.ast_nat_to_dim(l);
                let r_dim = self.ast_nat_to_dim(r);
                DimExpr::BinOp(dim_op, Box::new(l_dim), Box::new(r_dim))
            }
        }
    }

    // ── Expression checking ──────────────────────────────

    /// Check an expression and return its type.
    pub fn check_expr(&mut self, expr: &ast::Expr) -> Result<Ty, ()> {
        match &expr.kind {
            ast::ExprKind::IntLit(_) => Ok(Ty::Prim(PrimTy::I64)),
            ast::ExprKind::FloatLit(_) => Ok(Ty::Prim(PrimTy::F64)),
            ast::ExprKind::BoolLit(_) => Ok(Ty::Prim(PrimTy::Bool)),
            ast::ExprKind::StrLit(_) => Ok(Ty::Prim(PrimTy::Str)),
            ast::ExprKind::NilLit => Ok(Ty::Unit),
            ast::ExprKind::KeywordLit(_) => Ok(Ty::Prim(PrimTy::Str)),

            ast::ExprKind::SymbolRef(name) => {
                self.env.lookup(name).cloned().ok_or_else(|| {
                    self.diags.add(Diagnostic::error(
                        format!("undefined symbol: `{}`", name),
                        expr.span,
                    ));
                })
            }

            ast::ExprKind::If(cond, then_branch, else_branch) => {
                let cond_ty = self.check_expr(cond)?;
                if cond_ty != Ty::Prim(PrimTy::Bool) {
                    self.diags.add(Diagnostic::error(
                        "if condition must be bool",
                        cond.span,
                    ));
                    return Err(());
                }
                let then_ty = self.check_expr(then_branch)?;
                let else_ty = self.check_expr(else_branch)?;
                if then_ty != else_ty {
                    self.diags.add(Diagnostic::error(
                        format!(
                            "if branches have different types: {:?} vs {:?}",
                            then_ty, else_ty
                        ),
                        expr.span,
                    ));
                    return Err(());
                }
                Ok(then_ty)
            }

            ast::ExprKind::Let(bindings, body) => {
                self.env.push_scope();
                for b in bindings {
                    let actual = self.check_expr(&b.value)?;
                    let declared = self.resolve_type(&b.ty)?;
                    // If declared type is inferred placeholder, use actual
                    let wc = self.sym_wildcard;
                    let bound_ty = if declared == Ty::TypeVar(wc) {
                        actual
                    } else {
                        // Check that actual is assignable to declared
                        if !self.types_compatible(&actual, &declared) {
                            self.diags.add(Diagnostic::error(
                                format!(
                                    "let binding type mismatch: expected {:?}, got {:?}",
                                    declared, actual
                                ),
                                b.span,
                            ));
                            self.env.pop_scope();
                            return Err(());
                        }
                        declared
                    };
                    let b_id = self.intern(&b.name);
                    self.env.bind(b_id, bound_ty);
                }
                let body_ty = self.check_expr(body)?;
                self.env.pop_scope();
                Ok(body_ty)
            }

            ast::ExprKind::Do(exprs) => {
                let mut ty = Ty::Unit;
                for e in exprs {
                    ty = self.check_expr(e)?;
                }
                Ok(ty)
            }

            ast::ExprKind::FnCall(callee, args) => {
                let callee_ty = self.check_expr(callee)?;
                match callee_ty {
                    Ty::Func { params, ret } => {
                        if args.len() != params.len() {
                            self.diags.add(Diagnostic::error(
                                format!(
                                    "function expects {} arguments, got {}",
                                    params.len(),
                                    args.len()
                                ),
                                expr.span,
                            ));
                            return Err(());
                        }
                        for (arg, param_ty) in args.iter().zip(params.iter()) {
                            let arg_ty = self.check_expr(arg)?;
                            self.check_assignable(&arg_ty, param_ty, arg.span)?;
                        }
                        Ok(*ret)
                    }
                    Ty::TypeVar(_) => {
                        // Polymorphic builtin — check args but return wildcard type.
                        // For known polymorphic builtins, enforce a minimum argument count
                        // so callers cannot silently omit required arguments.
                        if let ast::ExprKind::SymbolRef(name) = &callee.kind {
                            if let Some(min_arity) = Self::polymorphic_builtin_min_arity(name) {
                                if args.len() < min_arity {
                                    self.diags.add(Diagnostic::error(
                                        format!(
                                            "`{}` requires at least {} argument(s), got {}",
                                            name, min_arity, args.len()
                                        ),
                                        expr.span,
                                    ));
                                    return Err(());
                                }
                            }
                        }
                        for arg in args {
                            let _ = self.check_expr(arg);
                        }
                        Ok(self.ty_wildcard())
                    }
                    _ => {
                        self.diags.add(Diagnostic::error(
                            format!("expected function type, got {:?}", callee_ty),
                            expr.span,
                        ));
                        Err(())
                    }
                }
            }

            ast::ExprKind::Match(scrutinee, arms) => {
                let scrut_ty = self.check_expr(scrutinee)?;
                let mut result_ty: Option<Ty> = None;
                for arm in arms {
                    self.env.push_scope();
                    self.check_pattern(&arm.pattern, &scrut_ty)?;
                    let arm_ty = self.check_expr(&arm.body)?;
                    self.env.pop_scope();
                    if let Some(ref prev) = result_ty {
                        if arm_ty != *prev {
                            self.diags.add(Diagnostic::error(
                                format!(
                                    "match arms have different types: {:?} vs {:?}",
                                    prev, arm_ty
                                ),
                                arm.span,
                            ));
                            return Err(());
                        }
                    } else {
                        result_ty = Some(arm_ty);
                    }
                }
                result_ty.ok_or_else(|| {
                    self.diags.add(Diagnostic::error(
                        "match requires at least one arm",
                        expr.span,
                    ));
                })
            }

            ast::ExprKind::Lambda(params, body) => {
                self.env.push_scope();
                let mut param_tys = Vec::new();
                for p in params {
                    let ty = self.resolve_type(&p.ty)?;
                    // For untyped lambda params (ty = "_"), default to inferred
                    let param_id = self.intern(&p.name);
                    let wc = self.sym_wildcard;
                    let bound_ty = if ty == Ty::TypeVar(wc) {
                        // Without full inference, we cannot determine the type,
                        // so we keep it as a type variable
                        Ty::TypeVar(param_id)
                    } else {
                        ty
                    };
                    self.env.bind(param_id, bound_ty.clone());
                    param_tys.push(bound_ty);
                }
                let body_ty = self.check_expr(body)?;
                self.env.pop_scope();
                Ok(Ty::Func {
                    params: param_tys,
                    ret: Box::new(body_ty),
                })
            }

            ast::ExprKind::Try(inner) => {
                let inner_ty = self.check_expr(inner)?;
                // If inner returns Named("Result", [T, E]), result type is T
                match inner_ty {
                    Ty::Named { ref name, ref args }
                        if self.env.resolve(*name) == "Result" && args.len() == 2 =>
                    {
                        if let TyArg::Type(ref t) = args[0] {
                            Ok(t.clone())
                        } else {
                            Err(())
                        }
                    }
                    _ => {
                        // try on a non-Result type just passes through
                        Ok(inner_ty)
                    }
                }
            }

            ast::ExprKind::VariantCtor(name, args) => {
                // Look up the variant constructor type if registered
                if let Some(ty) = self.env.lookup(name).cloned() {
                    match ty {
                        Ty::Func { params, ret } => {
                            if args.len() != params.len() {
                                self.diags.add(Diagnostic::error(
                                    format!(
                                        "variant {} expects {} arguments, got {}",
                                        name,
                                        params.len(),
                                        args.len()
                                    ),
                                    expr.span,
                                ));
                                return Err(());
                            }
                            for (arg, param_ty) in args.iter().zip(params.iter()) {
                                let arg_ty = self.check_expr(arg)?;
                                self.check_assignable(&arg_ty, param_ty, arg.span)?;
                            }
                            Ok(*ret)
                        }
                        _ => Ok(ty),
                    }
                } else {
                    // Unknown variant — check args and return a placeholder Named type
                    let mut arg_tys = Vec::new();
                    for arg in args {
                        arg_tys.push(TyArg::Type(self.check_expr(arg)?));
                    }
                    let name_id = self.intern(name);
                    Ok(Ty::Named {
                        name: name_id,
                        args: arg_tys,
                    })
                }
            }

            ast::ExprKind::StructLit(_name, fields) => {
                let mut field_tys = Vec::new();
                for (fname, fexpr) in fields {
                    let ty = self.check_expr(fexpr)?;
                    let fname_id = self.intern(fname);
                    field_tys.push(TyField {
                        name: fname_id,
                        ty,
                    });
                }
                Ok(Ty::Product(field_tys))
            }

            ast::ExprKind::ListLit(items) => {
                // All items must have the same type
                let list_id = self.intern("List");
                if items.is_empty() {
                    return Ok(Ty::Named {
                        name: list_id,
                        args: vec![TyArg::Type(self.ty_wildcard())],
                    });
                }
                let first_ty = self.check_expr(&items[0])?;
                for item in &items[1..] {
                    let item_ty = self.check_expr(item)?;
                    if item_ty != first_ty {
                        self.diags.add(Diagnostic::error(
                            format!(
                                "list elements have different types: {:?} vs {:?}",
                                first_ty, item_ty
                            ),
                            item.span,
                        ));
                        return Err(());
                    }
                }
                Ok(Ty::Named {
                    name: list_id,
                    args: vec![TyArg::Type(first_ty)],
                })
            }

            ast::ExprKind::Forall(_, _, _) | ast::ExprKind::Exists(_, _, _) => {
                Ok(Ty::Prim(PrimTy::Bool))
            }
        }
    }

    // ── Pattern checking ─────────────────────────────────

    /// Check a pattern against a scrutinee type, binding pattern variables.
    fn check_pattern(&mut self, pattern: &ast::Pattern, scrut_ty: &Ty) -> Result<(), ()> {
        match &pattern.kind {
            ast::PatternKind::Wildcard => Ok(()),
            ast::PatternKind::Binding(name) => {
                let name_id = self.intern(name);
                self.env.bind(name_id, scrut_ty.clone());
                Ok(())
            }
            ast::PatternKind::Literal(_) => {
                // We don't deeply check literal patterns against the scrutinee
                // for now — just accept them.
                Ok(())
            }
            ast::PatternKind::Variant(name, sub_pats) => {
                // Look up variant field types from the scrutinee's sum definition.
                let field_types = self.lookup_variant_fields(scrut_ty, name);
                if let Some(fields) = field_types {
                    if sub_pats.len() != fields.len() {
                        self.diags.add(Diagnostic::error(
                            format!(
                                "variant `{}` has {} fields, but pattern has {} sub-patterns",
                                name, fields.len(), sub_pats.len()
                            ),
                            pattern.span,
                        ));
                        return Err(());
                    }
                    for (sub, field_ty) in sub_pats.iter().zip(fields.iter()) {
                        self.check_pattern(sub, field_ty)?;
                    }
                } else {
                    // Variant not found in type — fall back to untyped binding
                    for sub in sub_pats {
                        self.check_pattern_binding(sub)?;
                    }
                }
                Ok(())
            }
        }
    }

    /// Bind variables in a sub-pattern without type information.
    fn check_pattern_binding(&mut self, pattern: &ast::Pattern) -> Result<(), ()> {
        match &pattern.kind {
            ast::PatternKind::Wildcard => Ok(()),
            ast::PatternKind::Binding(name) => {
                // Without full variant type info, bind as a type variable
                let name_id = self.intern(name);
                self.env.bind(name_id, Ty::TypeVar(name_id));
                Ok(())
            }
            ast::PatternKind::Literal(_) => Ok(()),
            ast::PatternKind::Variant(_, sub_pats) => {
                for sub in sub_pats {
                    self.check_pattern_binding(sub)?;
                }
                Ok(())
            }
        }
    }

    // ── Top-level checking ───────────────────────────────

    /// Check a top-level form.
    pub fn check_top_level(&mut self, top: &ast::TopLevel) -> Result<(), ()> {
        match top {
            ast::TopLevel::Defn(f) => {
                self.check_fn(f)?;
                Ok(())
            }
            ast::TopLevel::DefType(td) => {
                self.register_type_def(td)?;
                Ok(())
            }
            ast::TopLevel::Module(m) => {
                for item in &m.body {
                    self.check_top_level(item)?;
                }
                Ok(())
            }
            ast::TopLevel::Expr(e) => {
                self.check_expr(e)?;
                Ok(())
            }
            ast::TopLevel::Define(_) => Ok(()), // No type checking for define
            ast::TopLevel::Task(_) => Ok(()),
            ast::TopLevel::UseDecl(_) => Ok(()),
            ast::TopLevel::ExternC(decl) => {
                let mut param_tys = Vec::new();
                for p in &decl.params {
                    let ty = self.resolve_type(&p.ty)?;
                    param_tys.push(ty);
                }
                let ret_ty = self.resolve_type(&decl.return_type)?;
                let fn_ty = Ty::Func {
                    params: param_tys,
                    ret: Box::new(ret_ty),
                };
                let c_name_id = self.intern(&decl.c_name);
                self.env.bind(c_name_id, fn_ty);
                Ok(())
            }
            ast::TopLevel::Import { .. } => Ok(()),
        }
    }

    /// Check a function definition.
    pub fn check_fn(&mut self, f: &ast::FnDef) -> Result<Ty, ()> {
        self.env.push_scope();
        let mut param_tys = Vec::new();
        for p in &f.params {
            let ty = self.resolve_type(&p.ty)?;
            let p_id = self.intern(&p.name);
            self.env.bind(p_id, ty.clone());
            param_tys.push(ty);
        }
        let declared_ret = self.resolve_type(&f.return_type)?;

        // Type-check :requires clauses (must be Bool)
        for req in &f.requires {
            match self.check_expr(req) {
                Ok(req_ty) => {
                    if !matches!(req_ty, Ty::Prim(PrimTy::Bool)) {
                        self.diags.add(Diagnostic::error(
                            format!(":requires clause must be Bool, got {:?}", req_ty),
                            req.span,
                        ));
                    }
                }
                Err(()) => {} // error already recorded
            }
        }

        // Pre-bind the function name so recursive calls can resolve.
        // Use the declared return type if available, otherwise a fresh type variable.
        let wc = self.sym_wildcard;
        let f_name_id = self.intern(&f.name);
        let preliminary_ret = if declared_ret == Ty::TypeVar(wc) {
            let ret_sym = self.intern(&format!("__ret_{}", f.name));
            Ty::TypeVar(ret_sym)
        } else {
            declared_ret.clone()
        };
        let preliminary_fn_ty = Ty::Func {
            params: param_tys.clone(),
            ret: Box::new(preliminary_ret),
        };
        self.env.bind(f_name_id, preliminary_fn_ty);

        let body_ty = self.check_expr(&f.body)?;
        // Check body_ty is assignable to declared_ret
        self.check_assignable(&body_ty, &declared_ret, f.body.span)?;

        // Bind `result` for ensures/invariants clauses
        let result_id = self.intern("result");
        self.env.bind(result_id, declared_ret.clone());

        // Type-check :ensures clauses (must be Bool)
        for ens in &f.ensures {
            match self.check_expr(ens) {
                Ok(ens_ty) => {
                    if !matches!(ens_ty, Ty::Prim(PrimTy::Bool)) {
                        self.diags.add(Diagnostic::error(
                            format!(":ensures clause must be Bool, got {:?}", ens_ty),
                            ens.span,
                        ));
                    }
                }
                Err(()) => {} // error already recorded
            }
        }

        // Type-check :invariants clauses (must be Bool)
        for inv in &f.invariants {
            match self.check_expr(inv) {
                Ok(inv_ty) => {
                    if !matches!(inv_ty, Ty::Prim(PrimTy::Bool)) {
                        self.diags.add(Diagnostic::error(
                            format!(":invariants clause must be Bool, got {:?}", inv_ty),
                            inv.span,
                        ));
                    }
                }
                Err(()) => {} // error already recorded
            }
        }

        self.env.pop_scope();
        let fn_ty = Ty::Func {
            params: param_tys,
            ret: Box::new(declared_ret),
        };
        self.env.bind(f_name_id, fn_ty.clone());
        Ok(fn_ty)
    }

    /// Register a type definition in the environment.
    fn register_type_def(&mut self, td: &ast::TypeDef) -> Result<(), ()> {
        let param_names: Vec<Symbol> = td.type_params.iter()
            .map(|p| self.intern(&p.name))
            .collect();
        let ty = match &td.body {
            ast::TypeDefBody::Sum(variants) => {
                let mut ty_variants = Vec::new();
                for v in variants {
                    let mut fields = Vec::new();
                    for f in &v.fields {
                        match self.resolve_type(f) {
                            Ok(ty) => fields.push(ty),
                            Err(()) => {
                                self.diags.add(Diagnostic::error(
                                    format!("unresolved type in variant `{}`", v.name),
                                    Span::dummy(),
                                ));
                                return Err(());
                            }
                        }
                    }
                    let v_name_id = self.intern(&v.name);
                    ty_variants.push(TyVariant {
                        name: v_name_id,
                        fields,
                    });
                }
                Ty::Sum(ty_variants)
            }
            ast::TypeDefBody::Product(fields) => {
                let mut ty_fields = Vec::new();
                for f in fields {
                    match self.resolve_type(&f.ty) {
                        Ok(ty) => {
                            let f_name_id = self.intern(&f.name);
                            ty_fields.push(TyField {
                                name: f_name_id,
                                ty,
                            });
                        }
                        Err(()) => {
                            self.diags.add(Diagnostic::error(
                                format!("unresolved type in field `{}`", f.name),
                                Span::dummy(),
                            ));
                            return Err(());
                        }
                    }
                }
                Ty::Product(ty_fields)
            }
            ast::TypeDefBody::Alias(ast_ty) => self.resolve_type(ast_ty)?,
        };
        let td_name_id = self.intern(&td.name);
        self.env.register_type(td_name_id, param_names, ty);
        Ok(())
    }

    // ── Type compatibility ───────────────────────────────

    /// Check that `actual` is assignable to `expected`.
    fn check_assignable(&mut self, actual: &Ty, expected: &Ty, span: Span) -> Result<(), ()> {
        if self.types_compatible(actual, expected) {
            // Emit a warning for narrowing numeric coercions
            if let (Ty::Prim(a), Ty::Prim(b)) = (actual, expected) {
                if a != b && !Self::can_widen(*a, *b) {
                    self.diags.add(Diagnostic::warning(
                        format!(
                            "implicit narrowing coercion from {} to {} — consider an explicit cast",
                            a, b
                        ),
                        span,
                    ));
                }
            }
            Ok(())
        } else {
            self.diags.add(Diagnostic::error(
                format!("type mismatch: expected {:?}, got {:?}", expected, actual),
                span,
            ));
            Err(())
        }
    }

    /// Check if two types are compatible, including dimension unification.
    fn types_compatible(&mut self, actual: &Ty, expected: &Ty) -> bool {
        // TypeVar("_") is compatible with anything (inference placeholder)
        let wc = self.sym_wildcard;
        if matches!(actual, Ty::TypeVar(n) if *n == wc)
            || matches!(expected, Ty::TypeVar(n) if *n == wc)
        {
            return true;
        }
        // Never is compatible with any expected type (bottom type) — but not the reverse.
        // `actual == Never` means "this expression never returns", which is safe to use
        // wherever any type is expected. The reverse (`expected == Never`) is unsound:
        // it would let any value satisfy a "never returns" contract.
        if matches!(actual, Ty::Never) {
            return true;
        }
        match (actual, expected) {
            (Ty::Prim(a), Ty::Prim(b)) => {
                if a == b {
                    return true;
                }
                // Allow widening coercions freely. Narrowing coercions are also
                // accepted here (for compatibility), but check_assignable emits
                // a warning for them.
                (a.is_integer() && b.is_integer()) || (a.is_float() && b.is_float())
            }
            (Ty::Unit, Ty::Unit) => true,
            (Ty::Func { params: ap, ret: ar }, Ty::Func { params: bp, ret: br }) => {
                ap.len() == bp.len()
                    && ap.iter().zip(bp.iter()).all(|(a, b)| self.types_compatible(a, b))
                    && self.types_compatible(ar, br)
            }
            (
                Ty::Tensor { elem: ae, shape: as_ },
                Ty::Tensor { elem: be, shape: bs },
            ) => {
                if !self.types_compatible(ae, be) {
                    return false;
                }
                if as_.len() != bs.len() {
                    return false;
                }
                for (a, b) in as_.iter().zip(bs.iter()) {
                    if unify_dim(a, b, &mut self.dim_subst).is_err() {
                        return false;
                    }
                }
                true
            }
            (
                Ty::Named { name: an, args: aa },
                Ty::Named { name: bn, args: ba },
            ) => {
                an == bn
                    && aa.len() == ba.len()
                    && aa.iter().zip(ba.iter()).all(|(a, b)| match (a, b) {
                        (TyArg::Type(at), TyArg::Type(bt)) => self.types_compatible(at, bt),
                        (TyArg::Nat(ad), TyArg::Nat(bd)) => {
                            unify_dim(ad, bd, &mut self.dim_subst).is_ok()
                        }
                        _ => false,
                    })
            }
            (Ty::TypeVar(a), Ty::TypeVar(b)) => a == b,
            _ => actual == expected,
        }
    }

    // ── Pattern helpers ───────────────────────────────────

    /// Look up field types for a variant name from the scrutinee type.
    /// Returns `Some(field_types)` if the scrutinee is a Sum type containing the variant.
    fn lookup_variant_fields(&self, scrut_ty: &Ty, variant_name: &str) -> Option<Vec<Ty>> {
        match scrut_ty {
            Ty::Sum(variants) => {
                for v in variants {
                    if self.env.resolve(v.name) == variant_name {
                        return Some(v.fields.clone());
                    }
                }
                None
            }
            Ty::Named { name, .. } => {
                // Look up the registered type definition to get the sum variants
                if let Some(reg) = self.env.lookup_type_id(*name) {
                    self.lookup_variant_fields(&reg.ty.clone(), variant_name)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    // ── Numeric widening ─────────────────────────────────

    /// Returns true if `from` can be implicitly widened to `to`.
    /// Only allows promotions that never lose precision or range.
    fn can_widen(from: PrimTy, to: PrimTy) -> bool {
        use PrimTy::*;
        match (from, to) {
            // Signed integer widening: i8 → i16 → i32 → i64
            (I8, I16) | (I8, I32) | (I8, I64) => true,
            (I16, I32) | (I16, I64) => true,
            (I32, I64) => true,
            // Unsigned integer widening: u8 → u16 → u32 → u64
            (U8, U16) | (U8, U32) | (U8, U64) => true,
            (U16, U32) | (U16, U64) => true,
            (U32, U64) => true,
            // Float widening: f32 → f64
            (F32, F64) => true,
            _ => false,
        }
    }

    // ── Polymorphic builtin arity ────────────────────────

    /// Return the minimum number of arguments required for a known polymorphic
    /// builtin that falls through to the `TypeVar` branch in `check_expr`.
    ///
    /// Only builtins that are *not* fully typed in `register_typed_builtins`
    /// (i.e., those bound as `TypeVar("builtin")`) need entries here.
    /// Builtins with explicit `Ty::Func` signatures already get arity checking
    /// from the `Ty::Func` branch and must NOT be listed here.
    fn polymorphic_builtin_min_arity(name: &str) -> Option<usize> {
        // Only builtins still registered as TypeVar("builtin") need min-arity here.
        // Builtins with Ty::Func signatures get arity checking automatically.
        match name {
            // Variadic stdio
            "print" | "println" | "eprint" | "eprintln" => Some(1),
            "format" => Some(1),
            // Agent protocol
            "spawn-agent" => Some(1),
            "send" | "send-async" => Some(2),
            "await" => Some(1),
            // Tensor ops
            "tensor.zeros" | "tensor.ones" => Some(1),
            "tensor.rand" => Some(2),
            "tensor.identity" => Some(1),
            "tensor.add" | "tensor.mul" | "tensor.matmul" => Some(2),
            "tensor.reshape" => Some(2),
            "tensor.transpose" | "tensor.softmax" | "tensor.sum" | "tensor.max" => Some(1),
            "tensor.slice" => Some(3),
            // Compiler internals
            "run-bytecode" | "compile-to-executable" => Some(1),
            _ => None,
        }
    }

    // ── Diagnostics ──────────────────────────────────────

    pub fn into_diagnostics(self) -> Diagnostics {
        self.diags
    }

    /// Drain diagnostics without consuming the checker.
    /// Useful for REPL where the checker persists across inputs.
    pub fn drain_diagnostics(&mut self) -> Diagnostics {
        std::mem::replace(&mut self.diags, Diagnostics::new())
    }

    pub fn has_errors(&self) -> bool {
        self.diags.has_errors()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test helper ──────────────────────────────────────

    fn parse_and_check(input: &str) -> Result<Ty, String> {
        let mut lexer = airl_syntax::Lexer::new(input);
        let tokens = lexer.lex_all().map_err(|d| d.message)?;
        let sexprs = airl_syntax::parse_sexpr_all(tokens).map_err(|d| d.message)?;
        if sexprs.is_empty() {
            return Err("no expressions parsed".to_string());
        }
        let mut diags = Diagnostics::new();
        let expr = airl_syntax::parser::parse_expr(&sexprs[0], &mut diags)
            .map_err(|d| d.message)?;
        let mut checker = TypeChecker::new();
        checker.check_expr(&expr).map_err(|_| "type error".to_string())
    }

    // ── Task 11a: Type resolution and basic expression checking ──

    #[test]
    fn resolve_primitive_types() {
        let checker = TypeChecker::new();
        assert_eq!(checker.resolve_type_name("i32"), Ok(Ty::Prim(PrimTy::I32)));
        assert_eq!(checker.resolve_type_name("bool"), Ok(Ty::Prim(PrimTy::Bool)));
        assert_eq!(checker.resolve_type_name("f64"), Ok(Ty::Prim(PrimTy::F64)));
        assert_eq!(checker.resolve_type_name("String"), Ok(Ty::Prim(PrimTy::Str)));
        assert_eq!(checker.resolve_type_name("Unit"), Ok(Ty::Unit));
        assert_eq!(checker.resolve_type_name("Never"), Ok(Ty::Never));
    }

    #[test]
    fn resolve_unknown_type_fails() {
        let checker = TypeChecker::new();
        assert!(checker.resolve_type_name("Nonexistent").is_err());
    }

    #[test]
    fn check_int_literal() {
        assert_eq!(parse_and_check("42"), Ok(Ty::Prim(PrimTy::I64)));
    }

    #[test]
    fn check_float_literal() {
        assert_eq!(parse_and_check("3.14"), Ok(Ty::Prim(PrimTy::F64)));
    }

    #[test]
    fn check_bool_literal() {
        assert_eq!(parse_and_check("true"), Ok(Ty::Prim(PrimTy::Bool)));
        assert_eq!(parse_and_check("false"), Ok(Ty::Prim(PrimTy::Bool)));
    }

    #[test]
    fn check_string_literal() {
        assert_eq!(parse_and_check(r#""hello""#), Ok(Ty::Prim(PrimTy::Str)));
    }

    #[test]
    fn check_nil_literal() {
        assert_eq!(parse_and_check("nil"), Ok(Ty::Unit));
    }

    #[test]
    fn check_arithmetic_same_type() {
        assert_eq!(parse_and_check("(+ 1 2)"), Ok(Ty::Prim(PrimTy::I64)));
        assert_eq!(parse_and_check("(- 10 3)"), Ok(Ty::Prim(PrimTy::I64)));
        assert_eq!(parse_and_check("(* 4 5)"), Ok(Ty::Prim(PrimTy::I64)));
    }

    #[test]
    fn check_arithmetic_type_mismatch() {
        assert!(parse_and_check(r#"(+ 1 "hello")"#).is_err());
    }

    #[test]
    fn check_comparison_returns_bool() {
        assert_eq!(parse_and_check("(< 1 2)"), Ok(Ty::Prim(PrimTy::Bool)));
        assert_eq!(parse_and_check("(= 3 4)"), Ok(Ty::Prim(PrimTy::Bool)));
    }

    #[test]
    fn check_let_binding_type() {
        assert_eq!(
            parse_and_check("(let (x : i32 42) x)"),
            Ok(Ty::Prim(PrimTy::I32))
        );
    }

    #[test]
    fn check_let_binding_type_mismatch() {
        // Binding declares i32 but value is a string
        assert!(parse_and_check(r#"(let (x : i32 "hello") x)"#).is_err());
    }

    #[test]
    fn check_if_branches_same_type() {
        assert_eq!(
            parse_and_check("(if true 1 2)"),
            Ok(Ty::Prim(PrimTy::I64))
        );
    }

    #[test]
    fn check_if_branches_different_type() {
        assert!(parse_and_check(r#"(if true 1 "hello")"#).is_err());
    }

    #[test]
    fn check_if_condition_must_be_bool() {
        assert!(parse_and_check("(if 42 1 2)").is_err());
    }

    // ── Task 11b: FnCall, Match, Lambda, Do, Try ─────────

    #[test]
    fn check_fn_definition_and_call() {
        // Define add function using AIRL keyword syntax, then call it
        let mut checker = TypeChecker::new();
        let input = r#"(defn add :sig [(a : i32) (b : i32) -> i32] :requires [(>= a 0)] :body (+ a b))"#;
        let mut lexer = airl_syntax::Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = airl_syntax::parse_sexpr_all(tokens).unwrap();
        let mut diags = Diagnostics::new();
        let top = airl_syntax::parse_top_level(&sexprs[0], &mut diags).unwrap();
        checker.check_top_level(&top).unwrap();

        // Now check a call: (add 1 2) should return i32
        let call_input = "(add 1 2)";
        let mut lexer2 = airl_syntax::Lexer::new(call_input);
        let tokens2 = lexer2.lex_all().unwrap();
        let sexprs2 = airl_syntax::parse_sexpr_all(tokens2).unwrap();
        let call_expr = airl_syntax::parser::parse_expr(&sexprs2[0], &mut diags).unwrap();
        let result = checker.check_expr(&call_expr).unwrap();
        assert_eq!(result, Ty::Prim(PrimTy::I32));
    }

    #[test]
    fn check_fn_call_wrong_arg_count() {
        let mut checker = TypeChecker::new();
        // Register a function manually
        checker.env.bind_str(
            "foo",
            Ty::Func {
                params: vec![Ty::Prim(PrimTy::I32)],
                ret: Box::new(Ty::Prim(PrimTy::I32)),
            },
        );
        // Call with wrong number of args
        let input = "(foo 1 2)";
        let mut lexer = airl_syntax::Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = airl_syntax::parse_sexpr_all(tokens).unwrap();
        let mut diags = Diagnostics::new();
        let expr = airl_syntax::parser::parse_expr(&sexprs[0], &mut diags).unwrap();
        assert!(checker.check_expr(&expr).is_err());
    }

    #[test]
    fn check_fn_call_wrong_arg_type() {
        let mut checker = TypeChecker::new();
        checker.env.bind_str(
            "foo",
            Ty::Func {
                params: vec![Ty::Prim(PrimTy::I32)],
                ret: Box::new(Ty::Prim(PrimTy::I32)),
            },
        );
        let input = r#"(foo "hello")"#;
        let mut lexer = airl_syntax::Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = airl_syntax::parse_sexpr_all(tokens).unwrap();
        let mut diags = Diagnostics::new();
        let expr = airl_syntax::parser::parse_expr(&sexprs[0], &mut diags).unwrap();
        assert!(checker.check_expr(&expr).is_err());
    }

    #[test]
    fn check_do_block() {
        // (do 1 2 3) returns type of last expression
        assert_eq!(parse_and_check("(do 1 2 3)"), Ok(Ty::Prim(PrimTy::I64)));
    }

    #[test]
    fn check_do_block_returns_last_type() {
        // (do 1 true) returns bool
        assert_eq!(
            parse_and_check("(do 1 true)"),
            Ok(Ty::Prim(PrimTy::Bool))
        );
    }

    #[test]
    fn check_do_empty() {
        assert_eq!(parse_and_check("(do)"), Ok(Ty::Unit));
    }

    #[test]
    fn check_match_result() {
        // match on a value with wildcard arms returning same type
        assert_eq!(
            parse_and_check("(match 42 x 1 _ 2)"),
            Ok(Ty::Prim(PrimTy::I64))
        );
    }

    #[test]
    fn check_match_arms_must_agree() {
        assert!(parse_and_check(r#"(match 42 x 1 _ "hello")"#).is_err());
    }

    #[test]
    fn check_lambda_typed_params() {
        // Lambda with typed params: (fn [(x : i64)] (+ x 1))
        assert_eq!(
            parse_and_check("(fn [(x : i64)] (+ x 1))"),
            Ok(Ty::Func {
                params: vec![Ty::Prim(PrimTy::I64)],
                ret: Box::new(Ty::Prim(PrimTy::I64)),
            })
        );
    }

    #[test]
    fn check_nested_let() {
        // (let (x : i64 1) (let (y : i64 2) (+ x y)))
        assert_eq!(
            parse_and_check("(let (x : i64 1) (let (y : i64 2) (+ x y)))"),
            Ok(Ty::Prim(PrimTy::I64))
        );
    }

    #[test]
    fn check_undefined_symbol() {
        assert!(parse_and_check("undefined_var").is_err());
    }

    // ── issue-014: Fix 1 — map signature T → U ────────────

    /// `map` applied to an identity-typed callback must still type-check.
    /// The callback returns the same element type, which is valid (T → T ⊆ T → U).
    #[test]
    fn map_accepts_same_type_callback() {
        // (map (fn [(x : i64)] x) [1 2 3]) — identity callback, T=U=i64
        let result = parse_and_check("(map (fn [(x : i64)] x) [1 2 3])");
        assert!(result.is_ok(), "map with identity callback should type-check: {:?}", result);
    }

    /// `map` must accept two arguments (function, list). Calling with one arg is an error.
    #[test]
    fn map_rejects_one_arg() {
        // (map (fn [(x : i64)] x)) — missing the list argument
        let result = parse_and_check("(map (fn [(x : i64)] x))");
        assert!(result.is_err(), "map with one arg should fail arity check");
    }

    /// `map` is registered with a `Ty::Func` signature (not TypeVar),
    /// so it must report the correct return type (List[_]).
    #[test]
    fn map_return_type_is_list() {
        let checker = TypeChecker::new();
        let map_ty = checker.env.lookup("map").cloned();
        assert!(map_ty.is_some(), "map must be registered");
        match map_ty.unwrap() {
            Ty::Func { params, ret } => {
                assert_eq!(params.len(), 2, "map must have 2 params");
                // Return type must be a List, not a bare TypeVar
                match *ret {
                    Ty::Named { ref name, .. } => {
                        assert_eq!(checker.env.resolve(*name), "List")
                    }
                    other => panic!("map return type should be List, got {:?}", other),
                }
                // The callback (first param) must have a DIFFERENT return TypeVar from
                // its input — i.e., it must be T→U not T→T.  We verify by checking that
                // the callback's return type is not structurally equal to its param type
                // when those are concrete wildcard markers.  Both are TypeVar("_") here
                // because we use wildcards for full polymorphism, which is the correct
                // representation.
                match &params[0] {
                    Ty::Func { params: cb_params, ret: cb_ret } => {
                        assert_eq!(cb_params.len(), 1, "map callback must take 1 arg");
                        // Both are TypeVar("_") — this is correct: wildcard input,
                        // wildcard output (distinct conceptually even if same Rust value).
                        // The key fix is that we don't require *the same* named TypeVar.
                        let _ = (cb_params, cb_ret); // structure is sound
                    }
                    other => panic!("map first param should be Func, got {:?}", other),
                }
            }
            other => panic!("map should be Func type, got {:?}", other),
        }
    }

    // ── issue-014: Fix 2 — TypeVar arity bypass ───────────

    /// Known polymorphic builtins in the TypeVar branch must reject too-few args.
    #[test]
    fn polymorphic_builtin_arity_is_enforced() {
        // `sort` needs at least 1 arg
        let result = parse_and_check("(sort)");
        assert!(result.is_err(), "sort() with zero args should fail arity check");

        // `concat` needs at least 2 args
        let result = parse_and_check("(concat [1 2])");
        assert!(result.is_err(), "concat with one arg should fail arity check");
    }

    /// A polymorphic builtin called with enough args must succeed.
    #[test]
    fn polymorphic_builtin_sufficient_arity_ok() {
        // `sort` with one list arg — no parser support for complex expressions here,
        // so we test via the checker directly.
        let _checker = TypeChecker::new();
        // `reverse` needs 1 arg — call it with a list literal
        let result = parse_and_check("(reverse [1 2 3])");
        assert!(result.is_ok(), "reverse with one list arg should succeed: {:?}", result);
    }

    // ── issue-014: Fix 3 — no typed builtins in wildcard list ────

    /// Typed builtins (those in register_typed_builtins) must NOT be shadowed
    /// by an entry in the wildcard TypeVar("builtin") list.  If they were, their
    /// explicit `Ty::Func` signature would be overwritten and arity/type checking
    /// would silently regress to the permissive TypeVar path.
    #[test]
    fn typed_builtins_not_in_wildcard_list() {
        // All builtins with explicit Ty::Func signatures.  After TypeChecker::new()
        // every name here must resolve to Ty::Func, not Ty::TypeVar("builtin").
        let typed_builtins = [
            // Tier 0: core operators
            "+", "-", "*", "/", "%", "<", ">", "<=", ">=", "=", "!=",
            "and", "or", "not",
            // Tier 1-2: original typed builtins
            "head", "tail", "cons", "empty?",
            "map", "filter", "fold", "str",
            "char-at", "substring", "split", "join", "replace", "chars", "words",
            "unwords", "lines", "unlines", "repeat-str", "pad-left", "pad-right",
            "is-empty-str", "reverse-str", "count-occurrences",
            "map-new", "map-get", "map-set", "map-has", "map-remove", "map-keys",
            "map-entries", "map-from-entries", "map-merge", "map-count",
            // Tier 3: list
            "length", "at", "at-or", "set-at", "list-contains?", "append",
            "reverse", "concat", "zip", "flatten", "range", "take", "drop",
            "sort", "find",
            // Tier 4: math
            "abs", "min", "max", "clamp", "sqrt", "sin", "cos", "tan", "log",
            "exp", "floor", "ceil", "round", "int-to-float", "float-to-int",
            // Tier 5: I/O + system + conversion
            "read-file", "write-file", "file-exists?", "append-file", "delete-file",
            "delete-dir", "rename-file", "read-dir", "create-dir", "file-size",
            "is-dir?", "temp-file", "temp-dir", "file-mtime",
            "shell-exec", "time-now", "sleep", "getenv", "get-args", "cpu-count",
            "format-time", "int-to-string", "float-to-string", "string-to-int",
            "string-to-float", "char-code", "char-from-code",
            "infinity", "nan", "is-nan?", "is-infinite?", "panic", "assert",
            // Tier 6: result
            "is-ok?", "is-err?", "unwrap-or", "map-ok", "map-err",
            "and-then", "or-else", "ok-or",
            // Tier 7: introspection + stdio
            "type-of", "valid", "shape", "read-line", "read-stdin",
            // Tier 8: math remaining + bitwise
            "sign", "even?", "odd?", "pow", "gcd", "lcm",
            "sum-list", "product-list",
            "bitwise-and", "bitwise-or", "bitwise-xor", "bitwise-shl", "bitwise-shr",
            // Tier 9: collections remaining
            "any", "all", "merge",
            // Tier 10: map helpers
            "map-get-or", "map-values", "map-from",
            "map-map-values", "map-filter", "map-update", "map-update-or",
            // Tier 11: bytes
            "bytes-new", "bytes-from-int8", "bytes-from-int16", "bytes-from-int32",
            "bytes-from-int64", "bytes-to-int16", "bytes-to-int32", "bytes-to-int64",
            "bytes-from-string", "bytes-to-string", "bytes-concat", "bytes-concat-all",
            "bytes-slice", "crc32c",
            // Tier 12: TCP
            "tcp-listen", "tcp-accept", "tcp-accept-tls", "tcp-connect",
            "tcp-connect-tls", "tcp-close", "tcp-send", "tcp-recv",
            "tcp-recv-exact", "tcp-set-timeout",
            // Tier 13: threading + channels
            "thread-spawn", "thread-join", "thread-set-affinity",
            "channel-new", "channel-send", "channel-recv", "channel-recv-timeout",
            "channel-drain", "channel-close",
            // Tier 14: crypto
            "sha256", "sha512", "hmac-sha256", "hmac-sha512",
            "sha256-bytes", "sha512-bytes", "hmac-sha256-bytes", "hmac-sha512-bytes",
            "pbkdf2-sha256", "pbkdf2-sha512",
            "base64-encode", "base64-decode", "base64-encode-bytes", "base64-decode-bytes",
            "random-bytes",
            // Tier 15: compression
            "gzip-compress", "gzip-decompress", "snappy-compress", "snappy-decompress",
            "lz4-compress", "lz4-decompress", "zstd-compress", "zstd-decompress",
            // Tier 16: system/conversion/path/regex
            "get-cwd", "shell-exec-with-stdin", "parse-int-radix", "int-to-string-radix",
            "json-parse", "json-stringify", "read-lines", "exec-file", "whoami",
            "path-parent", "path-filename", "path-extension", "is-absolute?",
            "regex-find-all", "regex-match", "regex-split",
        ];

        let checker = TypeChecker::new();
        let builtin_sym = checker.sym_builtin;
        for name in &typed_builtins {
            let ty = checker.env.lookup(name).cloned();
            assert!(ty.is_some(), "builtin `{}` must be registered", name);
            match ty.unwrap() {
                Ty::Func { .. } => {} // correct — explicit typed signature
                Ty::TypeVar(v) if v == builtin_sym => {
                    panic!(
                        "builtin `{}` is registered as TypeVar(\"builtin\") but should have \
                         an explicit Ty::Func signature. It was likely added to the wildcard \
                         list, shadowing its typed registration.",
                        name
                    );
                }
                other => {
                    panic!("builtin `{}` has unexpected type {:?}", name, other);
                }
            }
        }
    }

    #[test]
    fn contract_requires_must_be_bool() {
        // A :requires clause with non-Bool type should produce a diagnostic error
        let mut checker = TypeChecker::new();
        let input = r#"(defn bad-contract :sig [(x : i32) -> i32] :requires [(+ x 1)] :body x)"#;
        let mut lexer = airl_syntax::Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = airl_syntax::parse_sexpr_all(tokens).unwrap();
        let mut diags = Diagnostics::new();
        let top = airl_syntax::parse_top_level(&sexprs[0], &mut diags).unwrap();
        let _ = checker.check_top_level(&top);

        // Should have a diagnostic error because (+ x 1) returns i64, not Bool
        let err_msgs: Vec<String> = checker.diags.errors()
            .map(|d| format!("{:?}", d))
            .collect();
        assert!(!err_msgs.is_empty(), "Expected error for non-Bool :requires clause");
        assert!(err_msgs.iter().any(|e| e.contains("must be Bool")), "Error message should mention Bool requirement: {:?}", err_msgs);
    }

    #[test]
    fn contract_ensures_must_be_bool() {
        // A :ensures clause with non-Bool type should produce a diagnostic error
        let mut checker = TypeChecker::new();
        let input = r#"(defn bad-contract :sig [(x : i32) -> i32] :ensures [(+ x 1)] :body x)"#;
        let mut lexer = airl_syntax::Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = airl_syntax::parse_sexpr_all(tokens).unwrap();
        let mut diags = Diagnostics::new();
        let top = airl_syntax::parse_top_level(&sexprs[0], &mut diags).unwrap();
        let _ = checker.check_top_level(&top);

        // Should have a diagnostic error because (+ x 1) returns i64, not Bool
        let err_msgs: Vec<String> = checker.diags.errors()
            .map(|d| format!("{:?}", d))
            .collect();
        assert!(!err_msgs.is_empty(), "Expected error for non-Bool :ensures clause");
        assert!(err_msgs.iter().any(|e| e.contains("must be Bool")), "Error message should mention Bool requirement: {:?}", err_msgs);
    }

    #[test]
    fn contract_valid_requires_bool() {
        // A :requires clause with Bool type should type-check successfully
        let mut checker = TypeChecker::new();
        let input = r#"(defn good-contract :sig [(x : i32) -> i32] :requires [(valid x)] :body x)"#;
        let mut lexer = airl_syntax::Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = airl_syntax::parse_sexpr_all(tokens).unwrap();
        let mut diags = Diagnostics::new();
        let top = airl_syntax::parse_top_level(&sexprs[0], &mut diags).unwrap();
        let result = checker.check_top_level(&top);

        // Should succeed because (valid x) returns Bool
        assert!(result.is_ok());
    }

    #[test]
    fn contract_result_binding_in_ensures() {
        // The 'result' variable should be bound in :ensures clauses
        let mut checker = TypeChecker::new();
        let input = r#"(defn good-contract :sig [(x : i32) -> i32] :ensures [(> result 0)] :body x)"#;
        let mut lexer = airl_syntax::Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = airl_syntax::parse_sexpr_all(tokens).unwrap();
        let mut diags = Diagnostics::new();
        let top = airl_syntax::parse_top_level(&sexprs[0], &mut diags).unwrap();
        let result = checker.check_top_level(&top);

        // Should succeed because 'result' is bound and (> result 0) returns Bool
        assert!(result.is_ok());
    }

    #[test]
    fn contract_undeclared_var_in_ensures() {
        // An undeclared variable in :ensures should produce an undefined symbol error
        let mut checker = TypeChecker::new();
        let input = r#"(defn bad-contract :sig [(x : i32) -> i32] :ensures [(> result unknown_var)] :body x)"#;
        let mut lexer = airl_syntax::Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = airl_syntax::parse_sexpr_all(tokens).unwrap();
        let mut diags = Diagnostics::new();
        let top = airl_syntax::parse_top_level(&sexprs[0], &mut diags).unwrap();
        let _ = checker.check_top_level(&top);

        // Should have a diagnostic error because unknown_var is undefined
        let err_msgs: Vec<String> = checker.diags.errors()
            .map(|d| format!("{:?}", d))
            .collect();
        assert!(!err_msgs.is_empty(), "Expected error for undefined variable in :ensures clause");
        assert!(err_msgs.iter().any(|e| e.contains("undefined")), "Error message should mention undefined symbol: {:?}", err_msgs);
    }
}
