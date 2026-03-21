use std::collections::HashMap;

use crate::identity::AgentId;
use crate::registry::AgentRegistry;
use crate::task::{TaskResult, TaskStatus};
use crate::transport::{Transport, TransportError};

/// Errors from the agent runtime.
#[derive(Debug)]
pub enum AgentError {
    Transport(TransportError),
    TaskNotFound(String),
    Protocol(String),
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentError::Transport(e) => write!(f, "agent transport error: {}", e),
            AgentError::TaskNotFound(id) => write!(f, "task not found: {}", id),
            AgentError::Protocol(msg) => write!(f, "protocol error: {}", msg),
        }
    }
}

impl std::error::Error for AgentError {}

impl From<TransportError> for AgentError {
    fn from(e: TransportError) -> Self {
        AgentError::Transport(e)
    }
}

/// The agent runtime manages identity, peer registry, and pending tasks.
///
/// Phase 1 keeps this deliberately simple — the full message loop and
/// task execution pipeline will be wired up when the driver is ready.
pub struct AgentRuntime {
    pub identity: AgentId,
    pub registry: AgentRegistry,
    pending: HashMap<String, TaskStatus>,
    next_task_id: u64,
}

impl AgentRuntime {
    pub fn new(identity: AgentId) -> Self {
        Self {
            identity,
            registry: AgentRegistry::new(),
            pending: HashMap::new(),
            next_task_id: 0,
        }
    }

    /// Generate a unique task ID for this agent.
    pub fn next_task_id(&mut self) -> String {
        let id = format!("{}-task-{}", self.identity.name, self.next_task_id);
        self.next_task_id += 1;
        id
    }

    /// Record a task as pending.
    pub fn track_pending(&mut self, task_id: String) {
        self.pending.insert(task_id, TaskStatus::Complete);
    }

    /// Mark a pending task as completed with the given result.
    pub fn complete_task(&mut self, task_id: &str, status: TaskStatus) -> Result<(), AgentError> {
        if self.pending.contains_key(task_id) {
            self.pending.insert(task_id.to_string(), status);
            Ok(())
        } else {
            Err(AgentError::TaskNotFound(task_id.to_string()))
        }
    }

    /// Check if a task is still tracked.
    pub fn task_status(&self, task_id: &str) -> Option<&TaskStatus> {
        self.pending.get(task_id)
    }

    /// Remove a completed task from the pending map.
    pub fn remove_task(&mut self, task_id: &str) -> Option<TaskStatus> {
        self.pending.remove(task_id)
    }

    /// Number of pending tasks.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::*;

    fn test_identity() -> AgentId {
        AgentId {
            name: "test-agent".into(),
            capabilities: vec![Capability::ComputeCpu],
            trust_level: TrustLevel::Verified,
            endpoint: Endpoint::Stdio,
        }
    }

    #[test]
    fn runtime_creation() {
        let rt = AgentRuntime::new(test_identity());
        assert_eq!(rt.identity.name, "test-agent");
        assert_eq!(rt.pending_count(), 0);
    }

    #[test]
    fn task_id_generation() {
        let mut rt = AgentRuntime::new(test_identity());
        assert_eq!(rt.next_task_id(), "test-agent-task-0");
        assert_eq!(rt.next_task_id(), "test-agent-task-1");
        assert_eq!(rt.next_task_id(), "test-agent-task-2");
    }

    #[test]
    fn track_and_complete_task() {
        let mut rt = AgentRuntime::new(test_identity());
        let id = rt.next_task_id();
        rt.track_pending(id.clone());
        assert_eq!(rt.pending_count(), 1);

        rt.complete_task(&id, TaskStatus::Complete).unwrap();
        assert_eq!(rt.task_status(&id), Some(&TaskStatus::Complete));

        let status = rt.remove_task(&id);
        assert_eq!(status, Some(TaskStatus::Complete));
        assert_eq!(rt.pending_count(), 0);
    }

    #[test]
    fn complete_nonexistent_task() {
        let mut rt = AgentRuntime::new(test_identity());
        let result = rt.complete_task("bogus", TaskStatus::Complete);
        assert!(result.is_err());
    }

    #[test]
    fn registry_integration() {
        let mut rt = AgentRuntime::new(test_identity());
        let peer = AgentId {
            name: "gpu-worker".into(),
            capabilities: vec![Capability::ComputeGpu],
            trust_level: TrustLevel::Verified,
            endpoint: Endpoint::Tcp("127.0.0.1:9000".parse().unwrap()),
        };
        rt.registry.register(peer);
        assert!(rt.registry.lookup("gpu-worker").is_some());
    }
}
