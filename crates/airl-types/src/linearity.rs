use airl_syntax::{Span, Diagnostic, Diagnostics};
use crate::ty::{Ty, is_copy};
use std::collections::HashMap;

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
    /// Maps binding names to their ownership state (scoped stack).
    states: Vec<HashMap<String, OwnershipState>>,
    diags: Diagnostics,
}

impl LinearityChecker {
    pub fn new() -> Self {
        Self {
            states: vec![HashMap::new()],
            diags: Diagnostics::new(),
        }
    }

    /// Introduce a new owned binding in the current scope.
    pub fn introduce(&mut self, name: String) {
        if let Some(scope) = self.states.last_mut() {
            scope.insert(name, OwnershipState::Owned);
        }
    }

    /// Track a move of the named binding. Marks it as Moved.
    pub fn track_move(&mut self, name: &str, span: Span) -> Result<(), ()> {
        let state = self.lookup_state(name);
        match state {
            Some(OwnershipState::Owned) => {
                self.set_state(name, OwnershipState::Moved { moved_at: span });
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
        let state = self.lookup_state(name);
        match (state, kind) {
            (Some(OwnershipState::Owned), BorrowKind::Immutable) => {
                self.set_state(
                    name,
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
                self.set_state(
                    name,
                    OwnershipState::Borrowed {
                        kind: BorrowKind::Immutable,
                        count: count + 1,
                    },
                );
                Ok(())
            }
            (Some(OwnershipState::Owned), BorrowKind::Mutable) => {
                self.set_state(
                    name,
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

    /// Release a borrow on the named binding (decrement count or return to Owned).
    pub fn release_borrow(&mut self, name: &str) {
        let state = self.lookup_state(name);
        match state {
            Some(OwnershipState::Borrowed { kind, count }) if count > 1 => {
                self.set_state(
                    name,
                    OwnershipState::Borrowed {
                        kind,
                        count: count - 1,
                    },
                );
            }
            Some(OwnershipState::Borrowed { .. }) => {
                self.set_state(name, OwnershipState::Owned);
            }
            _ => {}
        }
    }

    fn lookup_state(&self, name: &str) -> Option<OwnershipState> {
        for scope in self.states.iter().rev() {
            if let Some(s) = scope.get(name) {
                return Some(s.clone());
            }
        }
        None
    }

    fn set_state(&mut self, name: &str, state: OwnershipState) {
        for scope in self.states.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), state);
                return;
            }
        }
    }

    pub fn push_scope(&mut self) {
        self.states.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        if self.states.len() > 1 {
            self.states.pop();
        }
    }

    /// Snapshot current state for branch checking.
    pub fn snapshot(&self) -> Vec<HashMap<String, OwnershipState>> {
        self.states.clone()
    }

    /// Restore state from a snapshot.
    pub fn restore(&mut self, snap: Vec<HashMap<String, OwnershipState>>) {
        self.states = snap;
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
}
