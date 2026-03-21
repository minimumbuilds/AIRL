use crate::value::Value;
use crate::error::RuntimeError;
use airl_syntax::Span;
use std::collections::{HashMap, HashSet};

/// The kind of scope frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    Module,
    Function,
    Let,
    Match,
}

/// A binding slot in the environment, tracking ownership/move state.
#[derive(Debug, Clone)]
pub struct Slot {
    pub value: Value,
    pub moved: bool,
    pub moved_at: Option<Span>,
    pub immutable_borrows: u32,
    pub mutable_borrow: bool,
}

/// A single scope frame with its bindings.
#[derive(Debug)]
pub struct Frame {
    pub bindings: HashMap<String, Slot>,
    pub kind: FrameKind,
}

/// Runtime environment — a stack of scope frames.
#[derive(Debug)]
pub struct Env {
    frames: Vec<Frame>,
}

impl Env {
    /// Create a new environment with a single module-level frame.
    pub fn new() -> Self {
        Self {
            frames: vec![Frame {
                bindings: HashMap::new(),
                kind: FrameKind::Module,
            }],
        }
    }

    /// Push a new scope frame.
    pub fn push_frame(&mut self, kind: FrameKind) {
        self.frames.push(Frame {
            bindings: HashMap::new(),
            kind,
        });
    }

    /// Pop the top scope frame. Panics if only the module frame remains.
    pub fn pop_frame(&mut self) {
        if self.frames.len() <= 1 {
            panic!("cannot pop the module frame");
        }
        self.frames.pop();
    }

    /// Bind a name to a value in the current (top) frame.
    pub fn bind(&mut self, name: String, value: Value) {
        let frame = self.frames.last_mut().expect("no frames");
        frame.bindings.insert(name, Slot {
            value,
            moved: false,
            moved_at: None,
            immutable_borrows: 0,
            mutable_borrow: false,
        });
    }

    /// Look up a binding by name, searching from innermost to outermost frame.
    /// Returns an error if the value has been moved.
    pub fn get(&self, name: &str) -> Result<&Value, RuntimeError> {
        for frame in self.frames.iter().rev() {
            if let Some(slot) = frame.bindings.get(name) {
                if slot.moved {
                    return Err(RuntimeError::UseAfterMove {
                        name: name.to_string(),
                        span: slot.moved_at.unwrap_or_else(Span::dummy),
                    });
                }
                return Ok(&slot.value);
            }
        }
        Err(RuntimeError::UndefinedSymbol(name.to_string()))
    }

    /// Mark a binding as moved. Errors if the binding is currently borrowed.
    pub fn mark_moved(&mut self, name: &str, span: Span) -> Result<(), RuntimeError> {
        for frame in self.frames.iter_mut().rev() {
            if let Some(slot) = frame.bindings.get_mut(name) {
                if slot.immutable_borrows > 0 || slot.mutable_borrow {
                    return Err(RuntimeError::Custom(format!(
                        "cannot move `{}` — borrowed", name
                    )));
                }
                slot.moved = true;
                slot.moved_at = Some(span);
                return Ok(());
            }
        }
        Err(RuntimeError::UndefinedSymbol(name.to_string()))
    }

    /// Increment immutable borrow count. Errors if mutably borrowed or moved.
    pub fn borrow_immutable(&mut self, name: &str) -> Result<(), RuntimeError> {
        for frame in self.frames.iter_mut().rev() {
            if let Some(slot) = frame.bindings.get_mut(name) {
                if slot.moved {
                    return Err(RuntimeError::UseAfterMove {
                        name: name.to_string(),
                        span: slot.moved_at.unwrap_or_else(Span::dummy),
                    });
                }
                if slot.mutable_borrow {
                    return Err(RuntimeError::Custom(format!(
                        "cannot immutably borrow `{}` — already mutably borrowed", name
                    )));
                }
                slot.immutable_borrows += 1;
                return Ok(());
            }
        }
        Err(RuntimeError::UndefinedSymbol(name.to_string()))
    }

    /// Set mutable borrow. Errors if any borrows exist or moved.
    pub fn borrow_mutable(&mut self, name: &str) -> Result<(), RuntimeError> {
        for frame in self.frames.iter_mut().rev() {
            if let Some(slot) = frame.bindings.get_mut(name) {
                if slot.moved {
                    return Err(RuntimeError::UseAfterMove {
                        name: name.to_string(),
                        span: slot.moved_at.unwrap_or_else(Span::dummy),
                    });
                }
                if slot.immutable_borrows > 0 {
                    return Err(RuntimeError::Custom(format!(
                        "cannot mutably borrow `{}` — already immutably borrowed", name
                    )));
                }
                if slot.mutable_borrow {
                    return Err(RuntimeError::Custom(format!(
                        "cannot mutably borrow `{}` — already mutably borrowed", name
                    )));
                }
                slot.mutable_borrow = true;
                return Ok(());
            }
        }
        Err(RuntimeError::UndefinedSymbol(name.to_string()))
    }

    /// Release an immutable borrow (decrement count).
    pub fn release_immutable_borrow(&mut self, name: &str) {
        for frame in self.frames.iter_mut().rev() {
            if let Some(slot) = frame.bindings.get_mut(name) {
                if slot.immutable_borrows > 0 {
                    slot.immutable_borrows -= 1;
                }
                return;
            }
        }
    }

    /// Release a mutable borrow (clear flag).
    pub fn release_mutable_borrow(&mut self, name: &str) {
        for frame in self.frames.iter_mut().rev() {
            if let Some(slot) = frame.bindings.get_mut(name) {
                slot.mutable_borrow = false;
                return;
            }
        }
    }

    /// Iterate all bindings across all frames (innermost first).
    /// Returns (name, &Slot) pairs. Later bindings shadow earlier ones.
    pub fn iter_bindings(&self) -> Vec<(&str, &Slot)> {
        let mut seen = HashSet::new();
        let mut result = Vec::new();
        for frame in self.frames.iter().rev() {
            for (name, slot) in &frame.bindings {
                if seen.insert(name.as_str()) {
                    result.push((name.as_str(), slot));
                }
            }
        }
        result.sort_by_key(|(name, _)| *name);
        result
    }

    /// Get a mutable reference to a binding's value.
    pub fn get_mut(&mut self, name: &str) -> Result<&mut Value, RuntimeError> {
        for frame in self.frames.iter_mut().rev() {
            if let Some(slot) = frame.bindings.get_mut(name) {
                if slot.moved {
                    return Err(RuntimeError::UseAfterMove {
                        name: name.to_string(),
                        span: slot.moved_at.unwrap_or_else(Span::dummy),
                    });
                }
                return Ok(&mut slot.value);
            }
        }
        Err(RuntimeError::UndefinedSymbol(name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_env_has_module_frame() {
        let env = Env::new();
        assert_eq!(env.frames.len(), 1);
        assert_eq!(env.frames[0].kind, FrameKind::Module);
    }

    #[test]
    fn bind_and_get() {
        let mut env = Env::new();
        env.bind("x".into(), Value::Int(42));
        assert_eq!(*env.get("x").unwrap(), Value::Int(42));
    }

    #[test]
    fn undefined_symbol_error() {
        let env = Env::new();
        let err = env.get("nonexistent").unwrap_err();
        assert!(matches!(err, RuntimeError::UndefinedSymbol(ref s) if s == "nonexistent"));
    }

    #[test]
    fn push_pop_frame() {
        let mut env = Env::new();
        env.bind("x".into(), Value::Int(1));
        env.push_frame(FrameKind::Let);
        env.bind("y".into(), Value::Int(2));

        // Both visible
        assert_eq!(*env.get("x").unwrap(), Value::Int(1));
        assert_eq!(*env.get("y").unwrap(), Value::Int(2));

        env.pop_frame();

        // x still visible, y gone
        assert_eq!(*env.get("x").unwrap(), Value::Int(1));
        assert!(env.get("y").is_err());
    }

    #[test]
    fn inner_frame_shadows_outer() {
        let mut env = Env::new();
        env.bind("x".into(), Value::Int(1));
        env.push_frame(FrameKind::Function);
        env.bind("x".into(), Value::Int(2));
        assert_eq!(*env.get("x").unwrap(), Value::Int(2));
        env.pop_frame();
        assert_eq!(*env.get("x").unwrap(), Value::Int(1));
    }

    #[test]
    fn mark_moved_and_use_after_move() {
        let mut env = Env::new();
        env.bind("v".into(), Value::Str("hello".into()));
        let span = Span::new(10, 15, 2, 5);
        env.mark_moved("v", span).unwrap();

        let err = env.get("v").unwrap_err();
        match err {
            RuntimeError::UseAfterMove { name, span: s } => {
                assert_eq!(name, "v");
                assert_eq!(s.line, 2);
                assert_eq!(s.col, 5);
            }
            _ => panic!("expected UseAfterMove"),
        }
    }

    #[test]
    fn mark_moved_undefined_symbol() {
        let mut env = Env::new();
        let err = env.mark_moved("nope", Span::dummy()).unwrap_err();
        assert!(matches!(err, RuntimeError::UndefinedSymbol(_)));
    }

    #[test]
    fn get_mut_updates_value() {
        let mut env = Env::new();
        env.bind("x".into(), Value::Int(1));
        {
            let v = env.get_mut("x").unwrap();
            *v = Value::Int(99);
        }
        assert_eq!(*env.get("x").unwrap(), Value::Int(99));
    }

    #[test]
    fn get_mut_after_move_fails() {
        let mut env = Env::new();
        env.bind("x".into(), Value::Int(1));
        env.mark_moved("x", Span::dummy()).unwrap();
        assert!(env.get_mut("x").is_err());
    }

    #[test]
    fn get_mut_undefined_fails() {
        let mut env = Env::new();
        assert!(env.get_mut("x").is_err());
    }

    #[test]
    #[should_panic(expected = "cannot pop the module frame")]
    fn pop_module_frame_panics() {
        let mut env = Env::new();
        env.pop_frame();
    }

    #[test]
    fn nested_frames() {
        let mut env = Env::new();
        env.bind("a".into(), Value::Int(1));
        env.push_frame(FrameKind::Function);
        env.bind("b".into(), Value::Int(2));
        env.push_frame(FrameKind::Let);
        env.bind("c".into(), Value::Int(3));
        env.push_frame(FrameKind::Match);
        env.bind("d".into(), Value::Int(4));

        // All visible from innermost
        assert_eq!(*env.get("a").unwrap(), Value::Int(1));
        assert_eq!(*env.get("d").unwrap(), Value::Int(4));

        env.pop_frame(); // pop Match
        assert!(env.get("d").is_err());
        assert_eq!(*env.get("c").unwrap(), Value::Int(3));
    }

    #[test]
    fn borrow_immutable_succeeds() {
        let mut env = Env::new();
        env.bind("x".into(), Value::Int(42));
        env.borrow_immutable("x").unwrap();
        assert_eq!(*env.get("x").unwrap(), Value::Int(42));
    }

    #[test]
    fn borrow_mutable_blocks_immutable() {
        let mut env = Env::new();
        env.bind("x".into(), Value::Int(1));
        env.borrow_mutable("x").unwrap();
        assert!(env.borrow_immutable("x").is_err());
    }

    #[test]
    fn immutable_borrow_blocks_mutable() {
        let mut env = Env::new();
        env.bind("x".into(), Value::Int(1));
        env.borrow_immutable("x").unwrap();
        assert!(env.borrow_mutable("x").is_err());
    }

    #[test]
    fn multiple_immutable_borrows_ok() {
        let mut env = Env::new();
        env.bind("x".into(), Value::Int(1));
        env.borrow_immutable("x").unwrap();
        env.borrow_immutable("x").unwrap();
    }

    #[test]
    fn release_borrow_allows_mutable() {
        let mut env = Env::new();
        env.bind("x".into(), Value::Int(1));
        env.borrow_immutable("x").unwrap();
        env.release_immutable_borrow("x");
        env.borrow_mutable("x").unwrap();
    }

    #[test]
    fn borrow_moved_value_fails() {
        let mut env = Env::new();
        env.bind("x".into(), Value::Int(1));
        env.mark_moved("x", Span::dummy()).unwrap();
        assert!(env.borrow_immutable("x").is_err());
    }

    #[test]
    fn mark_moved_while_borrowed_fails() {
        let mut env = Env::new();
        env.bind("x".into(), Value::Int(1));
        env.borrow_immutable("x").unwrap();
        let err = env.mark_moved("x", Span::dummy()).unwrap_err();
        assert!(matches!(err, RuntimeError::Custom(ref s) if s.contains("borrowed")));
    }

    #[test]
    fn iter_bindings_returns_all() {
        let mut env = Env::new();
        env.bind("a".into(), Value::Int(1));
        env.bind("b".into(), Value::Int(2));
        let bindings = env.iter_bindings();
        let names: Vec<&str> = bindings.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, vec!["a", "b"]);
    }
}
