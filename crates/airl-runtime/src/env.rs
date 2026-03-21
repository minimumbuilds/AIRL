use crate::value::Value;
use crate::error::RuntimeError;
use airl_syntax::Span;
use std::collections::HashMap;

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

    /// Mark a binding as moved.
    pub fn mark_moved(&mut self, name: &str, span: Span) -> Result<(), RuntimeError> {
        for frame in self.frames.iter_mut().rev() {
            if let Some(slot) = frame.bindings.get_mut(name) {
                slot.moved = true;
                slot.moved_at = Some(span);
                return Ok(());
            }
        }
        Err(RuntimeError::UndefinedSymbol(name.to_string()))
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
}
