pub mod error;
pub mod value;
pub mod tensor;
pub mod env;
pub mod pattern;
pub mod builtins;
pub mod eval;
pub mod agent_client;

// Convenience re-exports
pub use error::RuntimeError;
pub use value::{Value, FnValue, LambdaValue};
pub use tensor::TensorValue;
pub use env::Env;
pub use pattern::try_match;
pub use builtins::Builtins;
pub use eval::Interpreter;
