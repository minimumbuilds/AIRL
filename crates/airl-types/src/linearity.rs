use airl_syntax::{Span, Diagnostic, Diagnostics};
use airl_syntax::ast::*;
use crate::ty::{Ty, is_copy, PrimTy};
use crate::interner::{SymbolId, SymbolInterner};
use std::collections::HashMap;
use std::rc::Rc;

/// Snapshot of linearity checker state for branch checking.
#[derive(Debug, Clone)]
pub struct LinearitySnapshot {
    shadow: Rc<HashMap<SymbolId, (OwnershipState, usize)>>,
    depth: usize,
    scope_bindings_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorrowKind {
    Immutable,
    Mutable,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OwnershipState {
    Owned,
    Borrowed { kind: BorrowKind, count: usize },
    Moved { moved_at: Span },
    Dropped,
}

/// Tracks ownership and borrowing state for bindings, enforcing linearity rules.
pub struct LinearityChecker {
    /// Current scope depth (0 = global).
    depth: usize,
    /// Flat shadow index: SymbolId -> (state, depth at which it was bound).
    /// Wrapped in Rc for O(1) snapshot/restore with clone-on-write mutations.
    shadow: Rc<HashMap<SymbolId, (OwnershipState, usize)>>,
    /// Per-depth list of (id, Option<previous_entry>) for rollback on pop.
    scope_bindings: Vec<Vec<(SymbolId, Option<(OwnershipState, usize)>)>>,
    /// Registry of known function parameter ownerships for call-site analysis.
    fn_ownerships: HashMap<SymbolId, Vec<Ownership>>,
    /// String interner for symbol names.
    interner: SymbolInterner,
    diags: Diagnostics,
}

impl LinearityChecker {
    pub fn new() -> Self {
        Self {
            depth: 0,
            shadow: Rc::new(HashMap::new()),
            scope_bindings: vec![Vec::new()],
            fn_ownerships: HashMap::new(),
            interner: SymbolInterner::new(),
            diags: Diagnostics::new(),
        }
    }

    /// Introduce a new owned binding in the current scope.
    pub fn introduce(&mut self, name: String) {
        let id = self.interner.intern(&name);
        let prev = Rc::make_mut(&mut self.shadow).insert(id, (OwnershipState::Owned, self.depth));
        if let Some(bindings) = self.scope_bindings.last_mut() {
            bindings.push((id, prev));
        }
    }

    /// Track a move of the named binding. Marks it as Moved.
    pub fn track_move(&mut self, name: &str, span: Span) -> Result<(), ()> {
        let id = self.interner.intern(name);
        let state = self.lookup_state_id(id);
        match state {
            Some(OwnershipState::Owned) => {
                self.set_state_id(id, OwnershipState::Moved { moved_at: span });
                Ok(())
            }
            Some(OwnershipState::Moved { moved_at }) => {
                self.diags.add(
                    Diagnostic::error(format!("use of moved value `{}`", name), span)
                        .with_note(moved_at, "value moved here"),
                );
                Err(())
            }
            Some(OwnershipState::Borrowed { .. }) => {
                self.diags.add(Diagnostic::error(
                    format!("cannot move `{}` while borrowed", name),
                    span,
                ));
                Err(())
            }
            _ => {
                self.diags.add(Diagnostic::error(
                    format!("use of undefined value `{}`", name),
                    span,
                ));
                Err(())
            }
        }
    }

    /// Track a borrow of the named binding.
    pub fn track_borrow(&mut self, name: &str, kind: BorrowKind, span: Span) -> Result<(), ()> {
        let id = self.interner.intern(name);
        let state = self.lookup_state_id(id);
        match (state, kind) {
            (Some(OwnershipState::Owned), BorrowKind::Immutable) => {
                self.set_state_id(
                    id,
                    OwnershipState::Borrowed {
                        kind: BorrowKind::Immutable,
                        count: 1,
                    },
                );
                Ok(())
            }
            (
                Some(OwnershipState::Borrowed {
                    kind: BorrowKind::Immutable,
                    count,
                }),
                BorrowKind::Immutable,
            ) => {
                self.set_state_id(
                    id,
                    OwnershipState::Borrowed {
                        kind: BorrowKind::Immutable,
                        count: count + 1,
                    },
                );
                Ok(())
            }
            (Some(OwnershipState::Owned), BorrowKind::Mutable) => {
                self.set_state_id(
                    id,
                    OwnershipState::Borrowed {
                        kind: BorrowKind::Mutable,
                        count: 1,
                    },
                );
                Ok(())
            }
            (Some(OwnershipState::Borrowed { .. }), BorrowKind::Mutable) => {
                self.diags.add(Diagnostic::error(
                    format!("cannot mutably borrow `{}` — already borrowed", name),
                    span,
                ));
                Err(())
            }
            (Some(OwnershipState::Moved { moved_at }), _) => {
                self.diags.add(
                    Diagnostic::error(format!("use of moved value `{}`", name), span)
                        .with_note(moved_at, "value moved here"),
                );
                Err(())
            }
            _ => Err(()),
        }
    }

    /// Track a copy of the named binding. Only succeeds if the type is Copy.
    pub fn track_copy(&mut self, _name: &str, ty: &Ty, span: Span) -> Result<(), ()> {
        if !is_copy(ty) {
            self.diags.add(Diagnostic::error(
                format!("type {:?} does not implement Copy", ty),
                span,
            ));
            return Err(());
        }
        // Copy doesn't change ownership state — the original remains Owned.
        Ok(())
    }

    // ── Static analysis: AST walking ─────────────────────

    /// Register a function's parameter ownership annotations.
    pub fn register_fn(&mut self, def: &FnDef) {
        let ownerships: Vec<Ownership> = def.params.iter()
            .map(|p| p.ownership)
            .collect();
        let id = self.interner.intern(&def.name);
        self.fn_ownerships.insert(id, ownerships);
    }

    /// Check a function definition for linearity violations.
    /// Introduces parameters, walks the body, and checks for issues.
    pub fn check_fn(&mut self, def: &FnDef) {
        // Register this function for call-site analysis
        self.register_fn(def);

        self.push_scope();

        // Introduce parameters based on their ownership annotation
        for param in &def.params {
            self.introduce(param.name.clone());
        }

        // Walk the body
        self.check_expr(&def.body);

        self.pop_scope();
    }

    /// Recursively walk an expression, tracking ownership state.
    fn check_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            // Atoms — no ownership effects
            ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::StrLit(_)
            | ExprKind::BoolLit(_) | ExprKind::NilLit | ExprKind::KeywordLit(_) => {}

            ExprKind::SymbolRef(_) => {
                // Reading a symbol is not a move by itself;
                // moves/borrows happen at call sites
            }

            ExprKind::FnCall(callee, args) => {
                // Get parameter ownership annotations if callee is a known function
                let param_ownerships = self.extract_callee_ownerships(callee, args.len());

                for (i, arg) in args.iter().enumerate() {
                    let ownership = param_ownerships.get(i).copied()
                        .unwrap_or(Ownership::Default);
                    self.check_arg(arg, ownership);
                }

                // Also check the callee itself (in case it's a complex expression)
                self.check_expr(callee);
            }

            ExprKind::Let(bindings, body) => {
                self.push_scope();
                for binding in bindings {
                    self.check_expr(&binding.value);
                    self.introduce(binding.name.clone());
                }
                self.check_expr(body);
                self.pop_scope();
            }

            ExprKind::Do(exprs) => {
                for e in exprs {
                    self.check_expr(e);
                }
            }

            ExprKind::If(cond, then_branch, else_branch) => {
                self.check_expr(cond);

                let snap = self.snapshot();

                // Check then branch
                self.check_expr(then_branch);
                let then_states = self.snapshot();

                // Restore and check else branch
                self.restore(snap.clone());
                self.check_expr(else_branch);
                let else_states = self.snapshot();

                // Merge: check that branches agree on ownership state
                self.merge_branch_states(&then_states, &else_states, expr.span);
            }

            ExprKind::Match(scrutinee, arms) => {
                self.check_expr(scrutinee);

                if arms.is_empty() {
                    return;
                }

                let snap = self.snapshot();
                let mut arm_states = Vec::new();

                for arm in arms {
                    self.restore(snap.clone());

                    // Pattern introduces bindings
                    self.push_scope();
                    self.introduce_pattern(&arm.pattern);
                    self.check_expr(&arm.body);
                    self.pop_scope();

                    arm_states.push(self.snapshot());
                }

                // Merge all arm states pairwise against the first
                if arm_states.len() > 1 {
                    for i in 1..arm_states.len() {
                        self.merge_branch_states(&arm_states[0], &arm_states[i], expr.span);
                    }
                }

                // Restore to first arm's state (representative post-match state)
                self.restore(arm_states.into_iter().next().unwrap_or(snap));
            }

            ExprKind::Lambda(_params, body) => {
                // Lambda captures current env but its body is checked independently
                self.push_scope();
                self.check_expr(body);
                self.pop_scope();
            }

            ExprKind::Try(inner) => {
                self.check_expr(inner);
            }

            ExprKind::VariantCtor(_, args) => {
                for a in args {
                    self.check_expr(a);
                }
            }

            ExprKind::StructLit(_, fields) => {
                for (_, val) in fields {
                    self.check_expr(val);
                }
            }

            ExprKind::ListLit(items) => {
                for item in items {
                    self.check_expr(item);
                }
            }

            ExprKind::Forall(_, where_c, body) | ExprKind::Exists(_, where_c, body) => {
                if let Some(guard) = where_c {
                    self.check_expr(guard);
                }
                self.check_expr(body);
            }
        }
    }

    /// Check an argument expression based on the parameter's ownership annotation.
    fn check_arg(&mut self, arg: &Expr, ownership: Ownership) {
        match (&arg.kind, ownership) {
            (ExprKind::SymbolRef(name), Ownership::Own) => {
                let _ = self.track_move(name, arg.span);
            }
            (ExprKind::SymbolRef(name), Ownership::Ref) => {
                let _ = self.track_borrow(name, BorrowKind::Immutable, arg.span);
            }
            (ExprKind::SymbolRef(name), Ownership::Mut) => {
                let _ = self.track_borrow(name, BorrowKind::Mutable, arg.span);
            }
            (ExprKind::SymbolRef(name), Ownership::Copy) => {
                // For static analysis, treat Copy as non-consuming
                let _ = self.track_copy(name, &Ty::Prim(PrimTy::I64), arg.span);
            }
            _ => {
                // Default ownership or complex expression — recurse
                self.check_expr(arg);
            }
        }
    }

    /// Extract parameter ownership annotations from a callee expression.
    /// Returns ownership list if callee is a known function with registered annotations.
    fn extract_callee_ownerships(&self, callee: &Expr, _arg_count: usize) -> Vec<Ownership> {
        if let ExprKind::SymbolRef(name) = &callee.kind {
            if let Some(id) = self.interner.get(name) {
                if let Some(ownerships) = self.fn_ownerships.get(&id) {
                    return ownerships.clone();
                }
            }
        }
        vec![]
    }

    /// Introduce bindings from a pattern into the current scope.
    fn introduce_pattern(&mut self, pattern: &Pattern) {
        match &pattern.kind {
            PatternKind::Binding(name) => {
                self.introduce(name.clone());
            }
            PatternKind::Variant(_, sub_pats) => {
                for p in sub_pats {
                    self.introduce_pattern(p);
                }
            }
            PatternKind::Wildcard | PatternKind::Literal(_) => {}
        }
    }

    /// Check that two branch states agree on ownership of all bindings.
    /// If a binding is Moved in one branch but Owned in another, emit an error.
    fn merge_branch_states(
        &mut self,
        state_a: &LinearitySnapshot,
        state_b: &LinearitySnapshot,
        span: Span,
    ) {
        // Compare all bindings in the flat shadow index
        for (id, (state_in_a, _)) in state_a.shadow.iter() {
            if let Some((state_in_b, _)) = state_b.shadow.get(id) {
                let a_moved = matches!(state_in_a, OwnershipState::Moved { .. });
                let b_moved = matches!(state_in_b, OwnershipState::Moved { .. });
                if a_moved != b_moved {
                    let name = self.interner.resolve(*id);
                    self.diags.add(Diagnostic::error(
                        format!(
                            "branches disagree on ownership of `{}`: moved in one branch but not the other",
                            name
                        ),
                        span,
                    ));
                }
            }
        }
    }

    /// Drain diagnostics without consuming the checker.
    pub fn drain_diagnostics(&mut self) -> Diagnostics {
        std::mem::replace(&mut self.diags, Diagnostics::new())
    }

    /// Release a borrow on the named binding (decrement count or return to Owned).
    pub fn release_borrow(&mut self, name: &str) {
        let id = self.interner.intern(name);
        let state = self.lookup_state_id(id);
        match state {
            Some(OwnershipState::Borrowed { kind, count }) if count > 1 => {
                self.set_state_id(
                    id,
                    OwnershipState::Borrowed {
                        kind,
                        count: count - 1,
                    },
                );
            }
            Some(OwnershipState::Borrowed { .. }) => {
                self.set_state_id(id, OwnershipState::Owned);
            }
            _ => {}
        }
    }

    /// Look up ownership state by string name (public API convenience).
    pub fn lookup_state(&self, name: &str) -> Option<OwnershipState> {
        let id = self.interner.get(name)?;
        self.shadow.get(&id).map(|(s, _)| s.clone())
    }

    fn lookup_state_id(&self, id: SymbolId) -> Option<OwnershipState> {
        self.shadow.get(&id).map(|(s, _)| s.clone())
    }

    fn set_state_id(&mut self, id: SymbolId, state: OwnershipState) {
        if let Some(entry) = Rc::make_mut(&mut self.shadow).get_mut(&id) {
            entry.0 = state;
        }
    }

    pub fn push_scope(&mut self) {
        self.depth += 1;
        self.scope_bindings.push(Vec::new());
    }

    pub fn pop_scope(&mut self) {
        if self.depth == 0 {
            return;
        }
        if let Some(bindings) = self.scope_bindings.pop() {
            let shadow = Rc::make_mut(&mut self.shadow);
            for (id, prev) in bindings.into_iter().rev() {
                match prev {
                    Some(entry) => { shadow.insert(id, entry); }
                    None => { shadow.remove(&id); }
                }
            }
        }
        self.depth -= 1;
    }

    /// Snapshot current state for branch checking.
    /// O(1): clones the Rc, not the HashMap.
    pub fn snapshot(&self) -> LinearitySnapshot {
        LinearitySnapshot {
            shadow: Rc::clone(&self.shadow),
            depth: self.depth,
            scope_bindings_len: self.scope_bindings.len(),
        }
    }

    /// Restore state from a snapshot.
    /// O(1): swaps the Rc pointer.
    /// Note: only scope_bindings *length* is restored (deeper layers are truncated).
    /// Callers must not introduce bindings at the current scope depth between
    /// snapshot() and restore() without an intervening push_scope().
    pub fn restore(&mut self, snap: LinearitySnapshot) {
        self.shadow = snap.shadow;
        self.depth = snap.depth;
        self.scope_bindings.truncate(snap.scope_bindings_len);
    }

    /// Consume the checker and return accumulated diagnostics.
    pub fn into_diagnostics(self) -> Diagnostics {
        self.diags
    }

    /// Whether any errors have been recorded.
    pub fn has_errors(&self) -> bool {
        self.diags.has_errors()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airl_syntax::Span;
    use crate::ty::{Ty, PrimTy};

    fn span(n: usize) -> Span {
        Span::new(n, n + 1, 1, n as u32)
    }

    #[test]
    fn valid_owned_use() {
        // Using an owned value once is fine.
        let mut lc = LinearityChecker::new();
        lc.introduce("x".into());
        assert!(lc.track_move("x", span(0)).is_ok());
        assert!(!lc.has_errors());
    }

    #[test]
    fn use_after_move() {
        // Move x, then try to use x again → error mentioning "moved".
        let mut lc = LinearityChecker::new();
        lc.introduce("x".into());
        assert!(lc.track_move("x", span(0)).is_ok());
        assert!(lc.track_move("x", span(5)).is_err());

        let diags = lc.into_diagnostics();
        assert!(diags.has_errors());
        let msg = &diags.errors().next().unwrap().message;
        assert!(msg.contains("moved"), "expected 'moved' in: {}", msg);
    }

    #[test]
    fn double_mut_borrow() {
        // Two sequential &mut borrows are ok: first ends before second starts.
        let mut lc = LinearityChecker::new();
        lc.introduce("x".into());
        assert!(lc.track_borrow("x", BorrowKind::Mutable, span(0)).is_ok());
        // Release the first borrow.
        lc.release_borrow("x");
        assert!(lc.track_borrow("x", BorrowKind::Mutable, span(5)).is_ok());
        assert!(!lc.has_errors());
    }

    #[test]
    fn mut_borrow_while_ref_exists() {
        // (&ref x) then (&mut x) simultaneously → error.
        let mut lc = LinearityChecker::new();
        lc.introduce("x".into());
        assert!(lc.track_borrow("x", BorrowKind::Immutable, span(0)).is_ok());
        // Attempt a mutable borrow while immutable borrow is active.
        assert!(lc.track_borrow("x", BorrowKind::Mutable, span(5)).is_err());
        assert!(lc.has_errors());
    }

    #[test]
    fn move_while_borrowed() {
        // Borrow x, then try to consume (move) x → error.
        let mut lc = LinearityChecker::new();
        lc.introduce("x".into());
        assert!(lc.track_borrow("x", BorrowKind::Immutable, span(0)).is_ok());
        assert!(lc.track_move("x", span(5)).is_err());

        let diags = lc.into_diagnostics();
        let msg = &diags.errors().next().unwrap().message;
        assert!(
            msg.contains("cannot move") && msg.contains("borrowed"),
            "expected move-while-borrowed error, got: {}",
            msg
        );
    }

    #[test]
    fn explicit_copy_allowed() {
        // Copy x (i32 is Copy), then use original — both succeed.
        let mut lc = LinearityChecker::new();
        lc.introduce("x".into());
        let i32_ty = Ty::Prim(PrimTy::I32);
        assert!(lc.track_copy("x", &i32_ty, span(0)).is_ok());
        // Original is still Owned, so we can move it.
        assert!(lc.track_move("x", span(5)).is_ok());
        assert!(!lc.has_errors());
    }

    #[test]
    fn branches_must_agree() {
        // Simulate: if one branch moves x and the other does not,
        // we detect disagreement.
        let mut lc = LinearityChecker::new();
        lc.introduce("x".into());

        // Snapshot before the branch.
        let snap = lc.snapshot();

        // Branch 1: moves x.
        lc.track_move("x", span(0)).ok();
        let branch1_state = lc.lookup_state("x");

        // Restore and run branch 2: does NOT move x.
        lc.restore(snap);
        let branch2_state = lc.lookup_state("x");

        // The branches disagree: one moved, the other didn't.
        assert_ne!(branch1_state, branch2_state, "branches should disagree on x's state");
    }

    #[test]
    fn nested_snapshot_restore_cow() {
        // Verify snapshot/restore with nested branches (if inside match inside if).
        // Each snapshot should be O(1) via Rc clone, and mutations should
        // clone-on-write without affecting other snapshots.
        let mut lc = LinearityChecker::new();
        lc.introduce("a".into());
        lc.introduce("b".into());
        lc.introduce("c".into());

        // Outer if: snapshot
        let outer_snap = lc.snapshot();
        assert_eq!(Rc::strong_count(&lc.shadow), 2, "snapshot should share Rc");

        // Then branch of outer if: move a
        lc.track_move("a", span(0)).ok();
        // After mutation, checker should have cloned away from snapshot
        assert_eq!(Rc::strong_count(&outer_snap.shadow), 1, "mutation should clone-on-write");

        // Inner match: snapshot inside then-branch
        let match_snap = lc.snapshot();

        // Match arm 1: move b
        lc.track_move("b", span(1)).ok();
        let arm1_state = lc.snapshot();

        // Match arm 2: restore to match_snap, move c instead
        lc.restore(match_snap.clone());
        assert!(lc.lookup_state("b") == Some(OwnershipState::Owned), "b should be Owned after restore");
        lc.track_move("c", span(2)).ok();
        let arm2_state = lc.snapshot();

        // Verify arms disagree on b and c (use interner to get SymbolIds)
        let b_id = lc.interner.get("b").unwrap();
        let c_id = lc.interner.get("c").unwrap();
        let b_in_arm1 = arm1_state.shadow.get(&b_id).map(|(s, _)| s.clone());
        let b_in_arm2 = arm2_state.shadow.get(&b_id).map(|(s, _)| s.clone());
        assert!(matches!(b_in_arm1, Some(OwnershipState::Moved { .. })));
        assert!(matches!(b_in_arm2, Some(OwnershipState::Owned)));

        let c_in_arm1 = arm1_state.shadow.get(&c_id).map(|(s, _)| s.clone());
        let c_in_arm2 = arm2_state.shadow.get(&c_id).map(|(s, _)| s.clone());
        assert!(matches!(c_in_arm1, Some(OwnershipState::Owned)));
        assert!(matches!(c_in_arm2, Some(OwnershipState::Moved { .. })));

        // Restore to outer snapshot: everything should be Owned again
        lc.restore(outer_snap);
        assert!(lc.lookup_state("a") == Some(OwnershipState::Owned));
        assert!(lc.lookup_state("b") == Some(OwnershipState::Owned));
        assert!(lc.lookup_state("c") == Some(OwnershipState::Owned));
    }

    #[test]
    fn snapshot_cow_no_clone_without_mutation() {
        // Verify that snapshot + restore without mutation never clones the HashMap.
        let mut lc = LinearityChecker::new();
        lc.introduce("x".into());

        let snap = lc.snapshot();
        // Both point to the same allocation
        assert!(Rc::ptr_eq(&lc.shadow, &snap.shadow));

        // Restore without mutation — still shared
        lc.restore(snap.clone());
        assert!(Rc::ptr_eq(&lc.shadow, &snap.shadow));
    }
}
