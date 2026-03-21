use melior::dialect::DialectRegistry;
use melior::ir::Location;
pub use melior::ir::Module;
use melior::utility::register_all_dialects;
pub use melior::Context;

/// Wrapper around `melior::Context` that registers all required MLIR dialects
/// (func, arith, memref, linalg, scf, cf, llvm) on construction.
pub struct MlirContext {
    ctx: Context,
}

impl MlirContext {
    /// Create a new MLIR context with all required dialects registered and loaded.
    pub fn new() -> Result<Self, String> {
        let registry = DialectRegistry::new();
        register_all_dialects(&registry);

        let ctx = Context::new();
        ctx.append_dialect_registry(&registry);
        ctx.load_all_available_dialects();

        Ok(Self { ctx })
    }

    /// Create a fresh MLIR module.
    pub fn new_module<'a>(&'a self) -> Module<'a> {
        let loc = Location::unknown(&self.ctx);
        Module::new(loc)
    }

    /// Access the underlying `melior::Context`.
    pub fn context(&self) -> &Context {
        &self.ctx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_creation() {
        let ctx = MlirContext::new().unwrap();
        assert!(ctx.context().loaded_dialect_count() > 0);
    }

    #[test]
    fn test_module_creation() {
        let ctx = MlirContext::new().unwrap();
        let module = ctx.new_module();
        assert!(module.as_operation().verify());
    }
}
