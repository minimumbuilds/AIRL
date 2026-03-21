//! Agent builtins — stub registrations for Phase 1.
//!
//! These will be wired into the interpreter when the full pipeline is ready.
//! For now, we define the names and signatures that the agent system will provide.

/// Names of agent builtins that will be registered with the interpreter.
pub const AGENT_BUILTINS: &[&str] = &[
    "send",
    "await",
    "spawn-agent",
    "parallel",
    "broadcast",
    "any-agent",
    "retry",
    "escalate",
];

/// Returns true if the given name is an agent builtin.
pub fn is_agent_builtin(name: &str) -> bool {
    AGENT_BUILTINS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_builtins() {
        assert!(is_agent_builtin("send"));
        assert!(is_agent_builtin("await"));
        assert!(is_agent_builtin("spawn-agent"));
        assert!(is_agent_builtin("parallel"));
        assert!(is_agent_builtin("broadcast"));
        assert!(is_agent_builtin("any-agent"));
        assert!(is_agent_builtin("retry"));
        assert!(is_agent_builtin("escalate"));
    }

    #[test]
    fn unknown_builtin() {
        assert!(!is_agent_builtin("not-a-builtin"));
        assert!(!is_agent_builtin(""));
    }

    #[test]
    fn builtin_count() {
        assert_eq!(AGENT_BUILTINS.len(), 8);
    }
}
