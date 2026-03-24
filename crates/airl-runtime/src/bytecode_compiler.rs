// crates/airl-runtime/src/bytecode_compiler.rs
use std::collections::HashMap;
use crate::ir::*;
use crate::value::Value;
use crate::bytecode::*;

pub struct BytecodeCompiler {
    instructions: Vec<Instruction>,
    constants: Vec<Value>,
    locals: HashMap<String, u16>,       // variable name → register slot
    next_reg: u16,
    max_reg: u16,
    lambda_counter: usize,              // unique lambda name counter
    lambda_prefix: String,              // prefix for lambda names (avoids cross-module collisions)
    compiled_lambdas: Vec<BytecodeFunc>, // lambdas compiled during expression compilation
    /// Maps function names to per-parameter ownership flags (true = Own, needs move tracking).
    ownership_map: HashMap<String, Vec<bool>>,
    /// Registers that have been marked as moved (need CheckNotMoved on subsequent loads).
    moved_regs: std::collections::HashSet<u16>,
}

impl BytecodeCompiler {
    pub fn new() -> Self {
        BytecodeCompiler {
            instructions: Vec::new(),
            constants: Vec::new(),
            locals: HashMap::new(),
            next_reg: 0,
            max_reg: 0,
            lambda_counter: 0,
            lambda_prefix: String::new(),
            compiled_lambdas: Vec::new(),
            ownership_map: HashMap::new(),
            moved_regs: std::collections::HashSet::new(),
        }
    }

    pub fn with_prefix(prefix: &str) -> Self {
        let mut c = Self::new();
        c.lambda_prefix = format!("{}_", prefix);
        c
    }

    /// Set the ownership map for move tracking during call compilation.
    pub fn set_ownership_map(&mut self, map: HashMap<String, Vec<bool>>) {
        self.ownership_map = map;
    }

    fn alloc_reg(&mut self) -> u16 {
        let r = self.next_reg;
        self.next_reg += 1;
        if self.next_reg > self.max_reg {
            self.max_reg = self.next_reg;
        }
        r
    }

    fn free_reg_to(&mut self, r: u16) {
        self.next_reg = r;
    }

    fn emit(&mut self, op: Op, dst: u16, a: u16, b: u16) {
        self.instructions.push(Instruction::new(op, dst, a, b));
    }

    fn add_constant(&mut self, val: Value) -> u16 {
        // Reuse existing constant if identical
        for (i, c) in self.constants.iter().enumerate() {
            if c == &val {
                return i as u16;
            }
        }
        let idx = self.constants.len() as u16;
        self.constants.push(val);
        idx
    }

    /// Collect free variables referenced in an IR node that are not bound locally.
    fn free_vars(node: &IRNode, bound: &std::collections::HashSet<String>, out: &mut Vec<String>) {
        match node {
            IRNode::Load(name) => {
                if !bound.contains(name) && !out.contains(name) {
                    out.push(name.clone());
                }
            }
            IRNode::Int(_) | IRNode::Float(_) | IRNode::Str(_) | IRNode::Bool(_) | IRNode::Nil => {}
            IRNode::If(c, t, e) => {
                Self::free_vars(c, bound, out);
                Self::free_vars(t, bound, out);
                Self::free_vars(e, bound, out);
            }
            IRNode::Do(exprs) => {
                for expr in exprs { Self::free_vars(expr, bound, out); }
            }
            IRNode::Let(bindings, body) => {
                let mut inner_bound = bound.clone();
                for b in bindings {
                    Self::free_vars(&b.expr, &inner_bound, out);
                    inner_bound.insert(b.name.clone());
                }
                Self::free_vars(body, &inner_bound, out);
            }
            IRNode::Call(name, args) => {
                // The call target may be a variable holding a function value
                // (e.g., a parameter like `transform` in map-map-values).
                // Treat it as a potential free variable — the lambda compiler
                // will filter to only names actually in locals.
                if !bound.contains(name) && !out.contains(name) {
                    out.push(name.clone());
                }
                for arg in args { Self::free_vars(arg, bound, out); }
            }
            IRNode::CallExpr(callee, args) => {
                Self::free_vars(callee, bound, out);
                for arg in args { Self::free_vars(arg, bound, out); }
            }
            IRNode::Lambda(params, body) => {
                let mut inner_bound = bound.clone();
                for p in params { inner_bound.insert(p.clone()); }
                Self::free_vars(body, &inner_bound, out);
            }
            IRNode::List(items) => {
                for item in items { Self::free_vars(item, bound, out); }
            }
            IRNode::Variant(_, args) => {
                for arg in args { Self::free_vars(arg, bound, out); }
            }
            IRNode::Match(scrutinee, arms) => {
                Self::free_vars(scrutinee, bound, out);
                for arm in arms {
                    let mut inner_bound = bound.clone();
                    Self::collect_pattern_bindings(&arm.pattern, &mut inner_bound);
                    Self::free_vars(&arm.body, &inner_bound, out);
                }
            }
            IRNode::Try(expr) => Self::free_vars(expr, bound, out),
            IRNode::Func(_, params, body) => {
                let mut inner_bound = bound.clone();
                for p in params { inner_bound.insert(p.clone()); }
                Self::free_vars(body, &inner_bound, out);
            }
        }
    }

    fn collect_pattern_bindings(pat: &IRPattern, bound: &mut std::collections::HashSet<String>) {
        match pat {
            IRPattern::Bind(name) => { bound.insert(name.clone()); }
            IRPattern::Variant(_, sub) => {
                for p in sub { Self::collect_pattern_bindings(p, bound); }
            }
            IRPattern::Wild | IRPattern::Lit(_) => {}
        }
    }

    /// Recursively bind pattern variables to registers, emitting MatchTag for nested variants.
    fn bind_pattern(&mut self, pat: &IRPattern, value_reg: u16) {
        match pat {
            IRPattern::Bind(name) => {
                self.locals.insert(name.clone(), value_reg);
            }
            IRPattern::Wild => {}
            IRPattern::Lit(_) => {} // Literal patterns in sub-positions don't bind
            IRPattern::Variant(tag, sub_pats) => {
                // Destructure nested variant: extract inner value
                let tag_idx = self.add_constant(Value::Str(tag.clone()));
                let inner_reg = self.alloc_reg();
                self.emit(Op::MatchTag, inner_reg, value_reg, tag_idx);
                if sub_pats.len() == 1 {
                    self.bind_pattern(&sub_pats[0], inner_reg);
                } else if sub_pats.len() > 1 {
                    self.bind_multi_field_pattern(sub_pats, inner_reg);
                }
            }
        }
    }

    /// Remove pattern bindings from locals (reverse of bind_pattern).
    fn unbind_pattern(&mut self, pat: &IRPattern) {
        match pat {
            IRPattern::Bind(name) => { self.locals.remove(name); }
            IRPattern::Wild | IRPattern::Lit(_) => {}
            IRPattern::Variant(_, sub_pats) => {
                for p in sub_pats { self.unbind_pattern(p); }
            }
        }
    }

    /// Bind multi-field variant sub-patterns.  The inner value is a list;
    /// emit `at(inner, i)` for each field that needs binding.
    fn bind_multi_field_pattern(&mut self, sub_pats: &[IRPattern], inner_reg: u16) {
        let at_idx = self.add_constant(Value::Str("at".into()));
        for (i, pat) in sub_pats.iter().enumerate() {
            match pat {
                IRPattern::Bind(name) => {
                    // Allocate 3 consecutive regs: result, arg0 (list), arg1 (index)
                    let call_dst = self.alloc_reg();
                    let _arg0 = self.alloc_reg(); // call_dst + 1
                    let _arg1 = self.alloc_reg(); // call_dst + 2
                    let idx_const = self.add_constant(Value::Int(i as i64));
                    self.emit(Op::Move, call_dst + 1, inner_reg, 0);
                    self.emit(Op::LoadConst, call_dst + 2, idx_const, 0);
                    self.emit(Op::CallBuiltin, call_dst, at_idx, 2);
                    self.locals.insert(name.clone(), call_dst);
                }
                IRPattern::Wild => {}
                IRPattern::Variant(tag, nested) => {
                    let call_dst = self.alloc_reg();
                    let _arg0 = self.alloc_reg();
                    let _arg1 = self.alloc_reg();
                    let idx_const = self.add_constant(Value::Int(i as i64));
                    self.emit(Op::Move, call_dst + 1, inner_reg, 0);
                    self.emit(Op::LoadConst, call_dst + 2, idx_const, 0);
                    self.emit(Op::CallBuiltin, call_dst, at_idx, 2);
                    let tag_idx = self.add_constant(Value::Str(tag.clone()));
                    let nested_inner = self.alloc_reg();
                    self.emit(Op::MatchTag, nested_inner, call_dst, tag_idx);
                    if nested.len() == 1 {
                        self.bind_pattern(&nested[0], nested_inner);
                    } else if nested.len() > 1 {
                        self.bind_multi_field_pattern(nested, nested_inner);
                    }
                }
                IRPattern::Lit(_) => {}
            }
        }
    }

    /// Remove multi-field pattern bindings from locals.
    fn unbind_multi_field_pattern(&mut self, sub_pats: &[IRPattern]) {
        for pat in sub_pats {
            match pat {
                IRPattern::Bind(name) => { self.locals.remove(name); }
                IRPattern::Wild | IRPattern::Lit(_) => {}
                IRPattern::Variant(_, nested) => {
                    self.unbind_multi_field_pattern(nested);
                }
            }
        }
    }

    /// Compile an IRNode expression, placing the result in `dst`.
    pub fn compile_expr(&mut self, node: &IRNode, dst: u16) {
        match node {
            IRNode::Int(v) => {
                let idx = self.add_constant(Value::Int(*v));
                self.emit(Op::LoadConst, dst, idx, 0);
            }
            IRNode::Float(v) => {
                let idx = self.add_constant(Value::Float(*v));
                self.emit(Op::LoadConst, dst, idx, 0);
            }
            IRNode::Str(s) => {
                let idx = self.add_constant(Value::Str(s.clone()));
                self.emit(Op::LoadConst, dst, idx, 0);
            }
            IRNode::Bool(true) => self.emit(Op::LoadTrue, dst, 0, 0),
            IRNode::Bool(false) => self.emit(Op::LoadFalse, dst, 0, 0),
            IRNode::Nil => self.emit(Op::LoadNil, dst, 0, 0),

            IRNode::Load(name) => {
                if let Some(&slot) = self.locals.get(name) {
                    // If this register was marked as moved, emit a check
                    if self.moved_regs.contains(&slot) {
                        let name_idx = self.add_constant(Value::Str(name.clone()));
                        self.emit(Op::CheckNotMoved, 0, slot, name_idx);
                    }
                    if slot != dst {
                        self.emit(Op::Move, dst, slot, 0);
                    }
                } else {
                    // Function ref or builtin — emit as IRFuncRef for CallReg resolution
                    let idx = self.add_constant(Value::IRFuncRef(name.clone()));
                    self.emit(Op::LoadConst, dst, idx, 0);
                }
            }

            IRNode::Do(exprs) => {
                if exprs.is_empty() {
                    self.emit(Op::LoadNil, dst, 0, 0);
                } else {
                    let save = self.next_reg;
                    for (i, expr) in exprs.iter().enumerate() {
                        if i == exprs.len() - 1 {
                            self.compile_expr(expr, dst);
                        } else {
                            let tmp = self.alloc_reg();
                            self.compile_expr(expr, tmp);
                        }
                    }
                    self.free_reg_to(save.max(dst + 1));
                }
            }

            IRNode::List(items) => {
                let start = self.next_reg;
                for item in items {
                    let r = self.alloc_reg();
                    self.compile_expr(item, r);
                }
                self.emit(Op::MakeList, dst, start, items.len() as u16);
                self.free_reg_to(start.max(dst + 1));
            }

            IRNode::If(cond, then_, else_) => {
                // Compile condition
                let cond_reg = self.alloc_reg();
                self.compile_expr(cond, cond_reg);
                // JumpIfFalse to else
                let jump_to_else = self.instructions.len();
                self.emit(Op::JumpIfFalse, 0, cond_reg, 0); // offset patched later
                self.free_reg_to(cond_reg.max(dst + 1));
                // Then branch
                self.compile_expr(then_, dst);
                let jump_to_end = self.instructions.len();
                self.emit(Op::Jump, 0, 0, 0); // offset patched later
                // Else branch
                let else_start = self.instructions.len();
                self.compile_expr(else_, dst);
                let end = self.instructions.len();
                // Patch jumps
                self.instructions[jump_to_else].b = (else_start as i16 - jump_to_else as i16 - 1) as u16;
                self.instructions[jump_to_end].a = (end as i16 - jump_to_end as i16 - 1) as u16;
            }

            IRNode::Let(bindings, body) => {
                let save_regs = self.next_reg;
                // Binding registers must not overlap with dst; start allocation above dst
                if self.next_reg <= dst {
                    self.next_reg = dst + 1;
                    if self.next_reg > self.max_reg { self.max_reg = self.next_reg; }
                }
                for binding in bindings {
                    let r = self.alloc_reg();
                    self.compile_expr(&binding.expr, r);
                    self.locals.insert(binding.name.clone(), r);
                }
                self.compile_expr(body, dst);
                // Remove bindings from locals
                for binding in bindings {
                    self.locals.remove(&binding.name);
                }
                self.free_reg_to(save_regs.max(dst + 1));
            }

            IRNode::Call(name, args) => {
                // Check if it's a known arithmetic/comparison builtin for direct opcodes
                let direct_op = match name.as_str() {
                    "+" => Some(Op::Add),
                    "-" => Some(Op::Sub),
                    "*" => Some(Op::Mul),
                    "/" => Some(Op::Div),
                    "%" => Some(Op::Mod),
                    "=" => Some(Op::Eq),
                    "!=" => Some(Op::Ne),
                    "<" => Some(Op::Lt),
                    "<=" => Some(Op::Le),
                    ">" => Some(Op::Gt),
                    ">=" => Some(Op::Ge),
                    "not" => Some(Op::Not),
                    _ => None,
                };

                if let Some(op) = direct_op {
                    if args.len() == 2 {
                        let a_reg = self.alloc_reg();
                        self.compile_expr(&args[0], a_reg);
                        let b_reg = self.alloc_reg();
                        self.compile_expr(&args[1], b_reg);
                        self.emit(op, dst, a_reg, b_reg);
                        self.free_reg_to(a_reg.max(dst + 1));
                    } else if args.len() == 1 {
                        let a_reg = self.alloc_reg();
                        self.compile_expr(&args[0], a_reg);
                        self.emit(op, dst, a_reg, 0);
                        self.free_reg_to(a_reg.max(dst + 1));
                    }
                } else {
                    // General function call
                    let is_local_call = self.locals.contains_key(name);
                    let callee_orig = if is_local_call {
                        Some(*self.locals.get(name).unwrap())
                    } else {
                        None
                    };

                    // Resolve ownership info for the callee
                    let ownership = self.ownership_map.get(name).cloned();

                    // Collect source registers for args that are variable loads
                    // (needed for ownership tracking after args are compiled)
                    let arg_source_regs: Vec<Option<u16>> = args.iter().map(|arg| {
                        if let IRNode::Load(var_name) = arg {
                            self.locals.get(var_name).copied()
                        } else {
                            None
                        }
                    }).collect();

                    // 1. Compile args to temp registers (safe, above all locals)
                    let save = self.next_reg;
                    let mut tmp_regs = Vec::new();
                    for arg in args {
                        let r = self.alloc_reg();
                        self.compile_expr(arg, r);
                        tmp_regs.push(r);
                    }

                    // 2. Save callee to a safe register (AFTER args compiled,
                    //    BEFORE moving args into call slots which may clobber it)
                    let safe_callee = if let Some(orig) = callee_orig {
                        let call_slot_end = dst + 1 + args.len() as u16;
                        if orig >= dst + 1 && orig < call_slot_end {
                            // Callee is in the danger zone — save it
                            let safe = self.alloc_reg();
                            self.emit(Op::Move, safe, orig, 0);
                            safe
                        } else {
                            orig // Safe as-is
                        }
                    } else {
                        0 // unused
                    };

                    // 3. Move temps into call slots [dst+1..]
                    for (i, &tmp) in tmp_regs.iter().enumerate() {
                        let slot = dst + 1 + i as u16;
                        if slot >= self.max_reg { self.max_reg = slot + 1; }
                        if tmp != slot {
                            self.emit(Op::Move, slot, tmp, 0);
                        }
                    }

                    // 4a. Ownership: check for borrow+move conflicts and emit CheckNotMoved
                    if let Some(ref own_flags) = ownership {
                        // Detect borrow+move conflict: same source register used for
                        // both an Own param and a non-Own (Ref/Mut) param
                        let mut own_regs = std::collections::HashSet::new();
                        let mut non_own_regs = std::collections::HashSet::new();
                        for (i, is_own) in own_flags.iter().enumerate() {
                            if let Some(Some(src_reg)) = arg_source_regs.get(i) {
                                if *is_own {
                                    own_regs.insert(*src_reg);
                                } else {
                                    non_own_regs.insert(*src_reg);
                                }
                            }
                        }
                        // If any register appears in both sets, emit a runtime error
                        // for the borrow+move conflict
                        for conflict_reg in own_regs.intersection(&non_own_regs) {
                            // Find the variable name for the error message
                            let var_name = args.iter().find_map(|arg| {
                                if let IRNode::Load(vn) = arg {
                                    if self.locals.get(vn) == Some(conflict_reg) {
                                        Some(vn.clone())
                                    } else { None }
                                } else { None }
                            }).unwrap_or_else(|| format!("register {}", conflict_reg));
                            // Emit error message constant that includes "borrowed"
                            let err_msg = format!("cannot move `{}` while it is borrowed in the same call", var_name);
                            let err_idx = self.add_constant(Value::Str(err_msg));
                            // Mark then check — guarantees the check will fail
                            self.emit(Op::MarkMoved, 0, *conflict_reg, 0);
                            self.emit(Op::CheckNotMoved, 0, *conflict_reg, err_idx);
                        }

                        // Emit CheckNotMoved for each Own param's source register
                        if own_regs.intersection(&non_own_regs).next().is_none() {
                            for (i, is_own) in own_flags.iter().enumerate() {
                                if *is_own {
                                    if let Some(Some(src_reg)) = arg_source_regs.get(i) {
                                        let var_name = args.get(i).and_then(|arg| {
                                            if let IRNode::Load(vn) = arg { Some(vn.clone()) } else { None }
                                        }).unwrap_or_else(|| format!("register {}", src_reg));
                                        let name_idx = self.add_constant(Value::Str(var_name));
                                        self.emit(Op::CheckNotMoved, 0, *src_reg, name_idx);
                                    }
                                }
                            }
                        }
                    }

                    // 4b. Emit call
                    if is_local_call {
                        self.emit(Op::CallReg, dst, safe_callee, args.len() as u16);
                    } else {
                        let name_idx = self.add_constant(Value::Str(name.clone()));
                        self.emit(Op::Call, dst, name_idx, args.len() as u16);
                    }

                    // 4c. Ownership: mark Own param source registers as moved
                    if let Some(ref own_flags) = ownership {
                        for (i, is_own) in own_flags.iter().enumerate() {
                            if *is_own {
                                if let Some(Some(src_reg)) = arg_source_regs.get(i) {
                                    self.emit(Op::MarkMoved, 0, *src_reg, 0);
                                    self.moved_regs.insert(*src_reg);
                                }
                            }
                        }
                    }

                    self.free_reg_to(save.max(dst + 1));
                }
            }

            IRNode::CallExpr(callee, args) => {
                let callee_reg = self.alloc_reg();
                self.compile_expr(callee, callee_reg);
                let save = self.next_reg;
                let mut tmp_regs = Vec::new();
                for arg in args {
                    let r = self.alloc_reg();
                    self.compile_expr(arg, r);
                    tmp_regs.push(r);
                }
                // Move temps into call slots [dst+1..]
                for (i, &tmp) in tmp_regs.iter().enumerate() {
                    let slot = dst + 1 + i as u16;
                    if slot >= self.max_reg { self.max_reg = slot + 1; }
                    if tmp != slot {
                        self.emit(Op::Move, slot, tmp, 0);
                    }
                }
                self.emit(Op::CallReg, dst, callee_reg, args.len() as u16);
                self.free_reg_to(save.max(dst + 1));
            }

            IRNode::Variant(tag, args) => {
                let tag_idx = self.add_constant(Value::Str(tag.clone()));
                if args.is_empty() {
                    self.emit(Op::MakeVariant0, dst, tag_idx, 0);
                } else if args.len() == 1 {
                    let a_reg = self.alloc_reg();
                    self.compile_expr(&args[0], a_reg);
                    self.emit(Op::MakeVariant, dst, tag_idx, a_reg);
                    self.free_reg_to(a_reg.max(dst + 1));
                } else {
                    // Multi-arg variant: wrap in list
                    let start = self.next_reg;
                    for arg in args {
                        let r = self.alloc_reg();
                        self.compile_expr(arg, r);
                    }
                    let list_reg = self.alloc_reg();
                    self.emit(Op::MakeList, list_reg, start, args.len() as u16);
                    self.emit(Op::MakeVariant, dst, tag_idx, list_reg);
                    self.free_reg_to(start.max(dst + 1));
                }
            }

            IRNode::Lambda(params, body) => {
                // Compile lambda body as a named function stored in a side table.
                let lambda_name = format!("__lambda_{}{}",  self.lambda_prefix, self.lambda_counter);
                self.lambda_counter += 1;

                // Only capture variables actually referenced in the lambda body (free variables).
                let mut param_set = std::collections::HashSet::new();
                for p in params { param_set.insert(p.clone()); }
                let mut free = Vec::new();
                Self::free_vars(body, &param_set, &mut free);

                // Filter to only variables that are in our current locals
                let captured_names: Vec<(String, u16)> = free.iter()
                    .filter_map(|name| self.locals.get(name).map(|&slot| (name.clone(), slot)))
                    .collect();

                let mut all_params: Vec<String> = captured_names.iter().map(|(n, _)| n.clone()).collect();
                all_params.extend(params.iter().cloned());

                let mut func = self.compile_function(&lambda_name, &all_params, body);
                func.capture_count = captured_names.len() as u16;
                self.compiled_lambdas.push(func);

                // Emit MakeClosure: copy captured values to consecutive regs, then emit opcode
                let capture_start = self.next_reg;
                for (_, slot) in &captured_names {
                    let r = self.alloc_reg();
                    self.emit(Op::Move, r, *slot, 0);
                }
                let name_idx = self.add_constant(Value::Str(lambda_name));
                self.emit(Op::MakeClosure, dst, name_idx, capture_start);
                self.free_reg_to(capture_start.max(dst + 1));
            }

            IRNode::Match(scrutinee, arms) => {
                let scr_reg = self.alloc_reg();
                self.compile_expr(scrutinee, scr_reg);

                let mut end_jumps = Vec::new();

                for arm in arms {
                    match &arm.pattern {
                        IRPattern::Wild => {
                            self.emit(Op::MatchWild, dst, scr_reg, 0);
                            // No jump needed — wildcard always matches
                            self.compile_expr(&arm.body, dst);
                        }
                        IRPattern::Bind(name) => {
                            // Bind scrutinee to name
                            self.locals.insert(name.clone(), scr_reg);
                            self.compile_expr(&arm.body, dst);
                            self.locals.remove(name);
                        }
                        IRPattern::Lit(val) => {
                            let val_reg = self.alloc_reg();
                            let idx = self.add_constant(val.clone());
                            self.emit(Op::LoadConst, val_reg, idx, 0);
                            self.emit(Op::Eq, val_reg, scr_reg, val_reg);
                            let skip = self.instructions.len();
                            self.emit(Op::JumpIfFalse, 0, val_reg, 0); // patch later
                            self.free_reg_to(val_reg.max(dst + 1));
                            self.compile_expr(&arm.body, dst);
                            end_jumps.push(self.instructions.len());
                            self.emit(Op::Jump, 0, 0, 0); // jump to end, patch later
                            let here = self.instructions.len();
                            self.instructions[skip].b = (here as i16 - skip as i16 - 1) as u16;
                        }
                        IRPattern::Variant(tag, sub_pats) => {
                            let tag_idx = self.add_constant(Value::Str(tag.clone()));
                            let inner_reg = self.alloc_reg();
                            self.emit(Op::MatchTag, inner_reg, scr_reg, tag_idx);
                            let skip = self.instructions.len();
                            self.emit(Op::JumpIfNoMatch, 0, 0, 0); // patch later
                            // Bind sub-patterns recursively
                            if sub_pats.len() == 1 {
                                self.bind_pattern(&sub_pats[0], inner_reg);
                                self.compile_expr(&arm.body, dst);
                                self.unbind_pattern(&sub_pats[0]);
                            } else if sub_pats.is_empty() {
                                self.compile_expr(&arm.body, dst);
                            } else {
                                // Multi-field variant: inner value is a list.
                                // Extract each field with `at(inner, i)` and bind.
                                self.bind_multi_field_pattern(sub_pats, inner_reg);
                                self.compile_expr(&arm.body, dst);
                                self.unbind_multi_field_pattern(sub_pats);
                            }
                            self.free_reg_to(inner_reg.max(dst + 1));
                            end_jumps.push(self.instructions.len());
                            self.emit(Op::Jump, 0, 0, 0); // jump to end
                            let here = self.instructions.len();
                            self.instructions[skip].a = (here as i16 - skip as i16 - 1) as u16;
                        }
                    }
                }
                // Patch all end jumps
                let end = self.instructions.len();
                for j in end_jumps {
                    self.instructions[j].a = (end as i16 - j as i16 - 1) as u16;
                }
                self.free_reg_to(scr_reg.max(dst + 1));
            }

            IRNode::Try(expr) => {
                let src = self.alloc_reg();
                self.compile_expr(expr, src);
                let _err_jump = self.instructions.len();
                self.emit(Op::TryUnwrap, dst, src, 0); // err_offset patched in context
                self.free_reg_to(src.max(dst + 1));
            }

            // Func nodes are handled at the program level, not as expressions
            IRNode::Func(_, _, _) => {
                self.emit(Op::LoadNil, dst, 0, 0);
            }
        }
    }

    /// Compile in tail position — emits TailCall for self-recursive calls.
    pub fn compile_expr_tail(&mut self, node: &IRNode, dst: u16, fn_name: &str) {
        match node {
            IRNode::Call(name, args) if name == fn_name => {
                // Self-recursive tail call.
                // Parallel-move safety: compile all args to temp registers first,
                // THEN move temps to r0..rN. This prevents clobbering a source
                // register that a later arg still needs (e.g., (f b a) where a=r0, b=r1).
                let save = self.next_reg;
                let mut tmps = Vec::new();
                for arg in args {
                    let tmp = self.alloc_reg();
                    self.compile_expr(arg, tmp);
                    tmps.push(tmp);
                }
                for (i, tmp) in tmps.iter().enumerate() {
                    if *tmp != i as u16 {
                        self.emit(Op::Move, i as u16, *tmp, 0);
                    }
                }
                self.free_reg_to(save);
                let name_idx = self.add_constant(Value::Str(fn_name.to_string()));
                self.emit(Op::TailCall, 0, name_idx, args.len() as u16);
            }
            IRNode::If(cond, then_, else_) => {
                // Propagate tail context to branches
                let cond_reg = self.alloc_reg();
                self.compile_expr(cond, cond_reg);
                let jump_to_else = self.instructions.len();
                self.emit(Op::JumpIfFalse, 0, cond_reg, 0);
                self.free_reg_to(cond_reg.max(dst + 1));
                self.compile_expr_tail(then_, dst, fn_name);
                let jump_to_end = self.instructions.len();
                self.emit(Op::Jump, 0, 0, 0);
                let else_start = self.instructions.len();
                self.compile_expr_tail(else_, dst, fn_name);
                let end = self.instructions.len();
                self.instructions[jump_to_else].b = (else_start as i16 - jump_to_else as i16 - 1) as u16;
                self.instructions[jump_to_end].a = (end as i16 - jump_to_end as i16 - 1) as u16;
            }
            IRNode::Do(exprs) if !exprs.is_empty() => {
                let save = self.next_reg;
                for (i, expr) in exprs.iter().enumerate() {
                    if i == exprs.len() - 1 {
                        self.compile_expr_tail(expr, dst, fn_name);
                    } else {
                        let tmp = self.alloc_reg();
                        self.compile_expr(expr, tmp);
                    }
                }
                self.free_reg_to(save.max(dst + 1));
            }
            IRNode::Let(bindings, body) => {
                let save = self.next_reg;
                for binding in bindings {
                    let r = self.alloc_reg();
                    self.compile_expr(&binding.expr, r);
                    self.locals.insert(binding.name.clone(), r);
                }
                self.compile_expr_tail(body, dst, fn_name);
                for binding in bindings {
                    self.locals.remove(&binding.name);
                }
                self.free_reg_to(save.max(dst + 1));
            }
            IRNode::Match(scrutinee, arms) => {
                // Propagate tail context into match arms
                let scr_reg = self.alloc_reg();
                self.compile_expr(scrutinee, scr_reg);
                let mut end_jumps = Vec::new();

                for arm in arms {
                    match &arm.pattern {
                        IRPattern::Wild => {
                            self.emit(Op::MatchWild, dst, scr_reg, 0);
                            self.compile_expr_tail(&arm.body, dst, fn_name);
                        }
                        IRPattern::Bind(name) => {
                            self.locals.insert(name.clone(), scr_reg);
                            self.compile_expr_tail(&arm.body, dst, fn_name);
                            self.locals.remove(name);
                        }
                        IRPattern::Lit(val) => {
                            let val_reg = self.alloc_reg();
                            let idx = self.add_constant(val.clone());
                            self.emit(Op::LoadConst, val_reg, idx, 0);
                            self.emit(Op::Eq, val_reg, scr_reg, val_reg);
                            let skip = self.instructions.len();
                            self.emit(Op::JumpIfFalse, 0, val_reg, 0);
                            self.free_reg_to(val_reg.max(dst + 1));
                            self.compile_expr_tail(&arm.body, dst, fn_name);
                            end_jumps.push(self.instructions.len());
                            self.emit(Op::Jump, 0, 0, 0);
                            let here = self.instructions.len();
                            self.instructions[skip].b = (here as i16 - skip as i16 - 1) as u16;
                        }
                        IRPattern::Variant(tag, sub_pats) => {
                            let tag_idx = self.add_constant(Value::Str(tag.clone()));
                            let inner_reg = self.alloc_reg();
                            self.emit(Op::MatchTag, inner_reg, scr_reg, tag_idx);
                            let skip = self.instructions.len();
                            self.emit(Op::JumpIfNoMatch, 0, 0, 0);
                            if sub_pats.len() == 1 {
                                self.bind_pattern(&sub_pats[0], inner_reg);
                                self.compile_expr_tail(&arm.body, dst, fn_name);
                                self.unbind_pattern(&sub_pats[0]);
                            } else if sub_pats.is_empty() {
                                self.compile_expr_tail(&arm.body, dst, fn_name);
                            } else {
                                self.bind_multi_field_pattern(sub_pats, inner_reg);
                                self.compile_expr_tail(&arm.body, dst, fn_name);
                                self.unbind_multi_field_pattern(sub_pats);
                            }
                            self.free_reg_to(inner_reg.max(dst + 1));
                            end_jumps.push(self.instructions.len());
                            self.emit(Op::Jump, 0, 0, 0);
                            let here = self.instructions.len();
                            self.instructions[skip].a = (here as i16 - skip as i16 - 1) as u16;
                        }
                    }
                }
                let end = self.instructions.len();
                for j in end_jumps {
                    self.instructions[j].a = (end as i16 - j as i16 - 1) as u16;
                }
                self.free_reg_to(scr_reg.max(dst + 1));
            }
            // Non-tail — delegate to regular compile
            _ => self.compile_expr(node, dst),
        }
    }

    /// Compile a top-level function definition.
    pub fn compile_function(&mut self, name: &str, params: &[String], body: &IRNode) -> BytecodeFunc {
        let mut compiler = BytecodeCompiler::new();
        // Inherit lambda counter and prefix to avoid name collisions
        compiler.lambda_counter = self.lambda_counter;
        compiler.lambda_prefix = self.lambda_prefix.clone();
        compiler.ownership_map = self.ownership_map.clone();
        // Bind params to first N registers
        for (i, param) in params.iter().enumerate() {
            compiler.locals.insert(param.clone(), i as u16);
            compiler.next_reg = (i as u16) + 1;
            compiler.max_reg = compiler.next_reg;
        }
        let dst = compiler.alloc_reg();
        compiler.compile_expr_tail(body, dst, name);
        compiler.emit(Op::Return, 0, dst, 0);

        // Transfer compiled lambdas and updated counter back to outer compiler
        self.lambda_counter = compiler.lambda_counter;
        self.compiled_lambdas.extend(compiler.compiled_lambdas.drain(..));

        BytecodeFunc {
            name: name.to_string(),
            arity: params.len() as u16,
            register_count: compiler.max_reg,
            capture_count: 0,
            instructions: compiler.instructions,
            constants: compiler.constants,
        }
    }

    /// Compile a list of top-level IRNodes into a list of BytecodeFuncs + a main function.
    pub fn compile_program(&mut self, nodes: &[IRNode]) -> (Vec<BytecodeFunc>, BytecodeFunc) {
        let mut functions = Vec::new();
        let mut main_nodes = Vec::new();

        for node in nodes {
            match node {
                IRNode::Func(name, params, body) => {
                    let func = self.compile_function(name, params, body);
                    functions.push(func);
                }
                _ => main_nodes.push(node.clone()),
            }
        }

        // Compile remaining top-level expressions as __main__
        let main_body = if main_nodes.is_empty() {
            IRNode::Nil
        } else if main_nodes.len() == 1 {
            main_nodes.into_iter().next().unwrap()
        } else {
            IRNode::Do(main_nodes)
        };

        let main_func = self.compile_function("__main__", &[], &main_body);
        // Collect any lambdas compiled during function/main compilation
        functions.extend(self.compiled_lambdas.drain(..));
        (functions, main_func)
    }

    /// Compile a function with contract checking (requires/ensures/invariants).
    /// Contract clauses are compiled as IR expressions and become assertion opcodes.
    /// `fn_name_for_error` is used in ContractViolation error messages.
    /// `param_names_for_bindings` maps register indices to param names for error reporting.
    pub fn compile_function_with_contracts(
        &mut self,
        name: &str,
        params: &[String],
        body: &IRNode,
        requires: &[(IRNode, String)],   // (clause_ir, clause_source_text)
        ensures: &[(IRNode, String)],
        invariants: &[(IRNode, String)],
    ) -> BytecodeFunc {
        let mut compiler = BytecodeCompiler::new();
        compiler.lambda_counter = self.lambda_counter;
        compiler.lambda_prefix = self.lambda_prefix.clone();
        compiler.ownership_map = self.ownership_map.clone();

        // Bind params to first N registers
        for (i, param) in params.iter().enumerate() {
            compiler.locals.insert(param.clone(), i as u16);
            compiler.next_reg = (i as u16) + 1;
            compiler.max_reg = compiler.next_reg;
        }

        // Store function name as a constant for error messages
        let fn_name_idx = compiler.add_constant(Value::Str(name.to_string()));

        // Compile :requires — check preconditions before body
        for (clause_ir, clause_source) in requires {
            let clause_src_idx = compiler.add_constant(Value::Str(clause_source.clone()));
            let bool_reg = compiler.alloc_reg();
            compiler.compile_expr(clause_ir, bool_reg);
            // Pack fn_name_idx into dst field for error reporting
            compiler.emit(Op::AssertRequires, fn_name_idx, bool_reg, clause_src_idx);
            compiler.free_reg_to(bool_reg);
        }

        // Compile body
        let dst = compiler.alloc_reg();
        compiler.compile_expr_tail(body, dst, name);

        // Bind "result" for ensures/invariant clauses
        compiler.locals.insert("result".to_string(), dst);

        // Compile :invariant — check after body, before ensures
        for (clause_ir, clause_source) in invariants {
            let clause_src_idx = compiler.add_constant(Value::Str(clause_source.clone()));
            let bool_reg = compiler.alloc_reg();
            compiler.compile_expr(clause_ir, bool_reg);
            compiler.emit(Op::AssertInvariant, fn_name_idx, bool_reg, clause_src_idx);
            compiler.free_reg_to(bool_reg);
        }

        // Compile :ensures — check postconditions
        for (clause_ir, clause_source) in ensures {
            let clause_src_idx = compiler.add_constant(Value::Str(clause_source.clone()));
            let bool_reg = compiler.alloc_reg();
            compiler.compile_expr(clause_ir, bool_reg);
            compiler.emit(Op::AssertEnsures, fn_name_idx, bool_reg, clause_src_idx);
            compiler.free_reg_to(bool_reg);
        }

        compiler.locals.remove("result");

        compiler.emit(Op::Return, 0, dst, 0);

        self.lambda_counter = compiler.lambda_counter;
        self.compiled_lambdas.extend(compiler.compiled_lambdas.drain(..));

        BytecodeFunc {
            name: name.to_string(),
            arity: params.len() as u16,
            register_count: compiler.max_reg,
            capture_count: 0,
            instructions: compiler.instructions,
            constants: compiler.constants,
        }
    }

    /// Compile a program with contract support. `contracts` maps function names to their
    /// (requires, ensures, invariants) clauses as (IR, source_text) pairs.
    pub fn compile_program_with_contracts(
        &mut self,
        nodes: &[IRNode],
        contracts: &HashMap<String, (Vec<(IRNode, String)>, Vec<(IRNode, String)>, Vec<(IRNode, String)>)>,
    ) -> (Vec<BytecodeFunc>, BytecodeFunc) {
        let mut functions = Vec::new();
        let mut main_nodes = Vec::new();

        for node in nodes {
            match node {
                IRNode::Func(name, params, body) => {
                    if let Some((req, ens, inv)) = contracts.get(name) {
                        let func = self.compile_function_with_contracts(name, params, body, req, ens, inv);
                        functions.push(func);
                    } else {
                        let func = self.compile_function(name, params, body);
                        functions.push(func);
                    }
                }
                _ => main_nodes.push(node.clone()),
            }
        }

        let main_body = if main_nodes.is_empty() {
            IRNode::Nil
        } else if main_nodes.len() == 1 {
            main_nodes.into_iter().next().unwrap()
        } else {
            IRNode::Do(main_nodes)
        };

        let main_func = self.compile_function("__main__", &[], &main_body);
        functions.extend(self.compiled_lambdas.drain(..));
        (functions, main_func)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_int() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Int(42), 0);
        assert_eq!(c.instructions.len(), 1);
        assert_eq!(c.instructions[0].op, Op::LoadConst);
        assert_eq!(c.constants[0], Value::Int(42));
    }

    #[test]
    fn test_compile_float() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Float(3.14), 0);
        assert_eq!(c.instructions.len(), 1);
        assert_eq!(c.instructions[0].op, Op::LoadConst);
        assert_eq!(c.constants[0], Value::Float(3.14));
    }

    #[test]
    fn test_compile_str() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Str("hello".to_string()), 0);
        assert_eq!(c.instructions.len(), 1);
        assert_eq!(c.instructions[0].op, Op::LoadConst);
        assert_eq!(c.constants[0], Value::Str("hello".to_string()));
    }

    #[test]
    fn test_compile_bool() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Bool(true), 0);
        assert_eq!(c.instructions[0].op, Op::LoadTrue);

        let mut c2 = BytecodeCompiler::new();
        c2.compile_expr(&IRNode::Bool(false), 0);
        assert_eq!(c2.instructions[0].op, Op::LoadFalse);
    }

    #[test]
    fn test_compile_nil() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Nil, 0);
        assert_eq!(c.instructions[0].op, Op::LoadNil);
    }

    #[test]
    fn test_compile_load_known() {
        let mut c = BytecodeCompiler::new();
        c.locals.insert("x".to_string(), 0);
        c.next_reg = 1;
        c.max_reg = 1;
        // Load x into reg 1 — should emit Move
        c.compile_expr(&IRNode::Load("x".to_string()), 1);
        assert_eq!(c.instructions[0].op, Op::Move);
        assert_eq!(c.instructions[0].dst, 1);
        assert_eq!(c.instructions[0].a, 0);
    }

    #[test]
    fn test_compile_load_same_reg() {
        let mut c = BytecodeCompiler::new();
        c.locals.insert("x".to_string(), 0);
        c.next_reg = 1;
        c.max_reg = 1;
        // Load x into reg 0 — already there, no Move needed
        c.compile_expr(&IRNode::Load("x".to_string()), 0);
        assert_eq!(c.instructions.len(), 0);
    }

    #[test]
    fn test_compile_load_unknown() {
        let mut c = BytecodeCompiler::new();
        // Unknown variable: emits IRFuncRef for CallReg resolution
        c.compile_expr(&IRNode::Load("foo".to_string()), 0);
        assert_eq!(c.instructions[0].op, Op::LoadConst);
        assert_eq!(c.constants[0], Value::IRFuncRef("foo".to_string()));
    }

    #[test]
    fn test_compile_do() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Do(vec![IRNode::Int(1), IRNode::Int(2)]), 0);
        // Should have LoadConst for 1 (temp), LoadConst for 2 (dst=0)
        assert!(c.instructions.len() >= 2);
    }

    #[test]
    fn test_compile_do_empty() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Do(vec![]), 0);
        assert_eq!(c.instructions[0].op, Op::LoadNil);
    }

    #[test]
    fn test_compile_do_single() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Do(vec![IRNode::Int(99)]), 0);
        assert_eq!(c.instructions.len(), 1);
        assert_eq!(c.instructions[0].op, Op::LoadConst);
        assert_eq!(c.constants[0], Value::Int(99));
    }

    #[test]
    fn test_compile_list() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::List(vec![IRNode::Int(1), IRNode::Int(2), IRNode::Int(3)]), 0);
        // Last instruction should be MakeList
        let last = c.instructions.last().unwrap();
        assert_eq!(last.op, Op::MakeList);
        assert_eq!(last.b, 3); // count = 3
    }

    #[test]
    fn test_compile_list_empty() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::List(vec![]), 0);
        let last = c.instructions.last().unwrap();
        assert_eq!(last.op, Op::MakeList);
        assert_eq!(last.b, 0);
    }

    #[test]
    fn test_compile_function() {
        let mut c = BytecodeCompiler::new();
        // (defn id [x] x)
        let func = c.compile_function("id", &["x".to_string()], &IRNode::Load("x".into()));
        assert_eq!(func.name, "id");
        assert_eq!(func.arity, 1);
        // Should have Move (x→dst) + Return, or just Return if x is already dst
        assert!(func.instructions.len() >= 1);
        // Last instruction must be Return
        assert_eq!(func.instructions.last().unwrap().op, Op::Return);
    }

    #[test]
    fn test_compile_function_literal_body() {
        let mut c = BytecodeCompiler::new();
        // (defn const42 [] 42)
        let func = c.compile_function("const42", &[], &IRNode::Int(42));
        assert_eq!(func.arity, 0);
        assert_eq!(func.instructions[0].op, Op::LoadConst);
        assert_eq!(func.constants[0], Value::Int(42));
        assert_eq!(func.instructions.last().unwrap().op, Op::Return);
    }

    #[test]
    fn test_constant_deduplication() {
        let mut c = BytecodeCompiler::new();
        // Adding the same constant twice should reuse the same slot
        let idx1 = c.add_constant(Value::Int(10));
        let idx2 = c.add_constant(Value::Int(10));
        assert_eq!(idx1, idx2);
        assert_eq!(c.constants.len(), 1);
    }

    #[test]
    fn test_compile_program_no_funcs() {
        let mut c = BytecodeCompiler::new();
        let nodes = vec![IRNode::Int(1), IRNode::Int(2)];
        let (funcs, main) = c.compile_program(&nodes);
        assert!(funcs.is_empty());
        assert_eq!(main.name, "__main__");
    }

    #[test]
    fn test_compile_program_with_func() {
        let mut c = BytecodeCompiler::new();
        let nodes = vec![
            IRNode::Func(
                "double".to_string(),
                vec!["x".to_string()],
                Box::new(IRNode::Load("x".to_string())),
            ),
            IRNode::Int(42),
        ];
        let (funcs, main) = c.compile_program(&nodes);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "double");
        assert_eq!(main.name, "__main__");
    }

    #[test]
    fn test_compile_if() {
        let mut c = BytecodeCompiler::new();
        let node = IRNode::If(
            Box::new(IRNode::Bool(true)),
            Box::new(IRNode::Int(1)),
            Box::new(IRNode::Int(2)),
        );
        c.compile_expr(&node, 0);
        // Should have: LoadTrue, JumpIfFalse, LoadConst(1), Jump, LoadConst(2)
        assert!(c.instructions.len() >= 5);
    }

    #[test]
    fn test_compile_let() {
        let mut c = BytecodeCompiler::new();
        let node = IRNode::Let(
            vec![IRBinding { name: "x".into(), expr: IRNode::Int(42) }],
            Box::new(IRNode::Load("x".into())),
        );
        c.compile_expr(&node, 0);
        assert!(c.instructions.len() >= 2);
    }

    #[test]
    fn test_compile_call_add() {
        let mut c = BytecodeCompiler::new();
        let node = IRNode::Call("+".into(), vec![IRNode::Int(3), IRNode::Int(4)]);
        c.compile_expr(&node, 0);
        // Should use direct Add opcode, not CallBuiltin
        let has_add = c.instructions.iter().any(|i| i.op == Op::Add);
        assert!(has_add, "arithmetic should compile to direct opcode");
    }
}
