use crate::identity::{AgentId, Capability};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Registry of known agents, supporting lookup by name or capability.
/// The inner map is wrapped in Arc<RwLock<>> for safe sharing across threads.
#[derive(Clone)]
pub struct AgentRegistry {
    agents: Arc<RwLock<HashMap<String, AgentId>>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register an agent. Overwrites any existing agent with the same name.
    pub fn register(&self, agent: AgentId) {
        self.agents.write().unwrap().insert(agent.name.clone(), agent);
    }

    /// Look up an agent by name.
    pub fn lookup(&self, name: &str) -> Option<AgentId> {
        self.agents.read().unwrap().get(name).cloned()
    }

    /// Find all agents that possess ALL of the requested capabilities.
    pub fn find_by_capability(&self, caps: &[Capability]) -> Vec<AgentId> {
        self.agents
            .read()
            .unwrap()
            .values()
            .filter(|agent| caps.iter().all(|c| agent.capabilities.contains(c)))
            .cloned()
            .collect()
    }

    /// Find any single agent that has all requested capabilities.
    pub fn find_any(&self, caps: &[Capability]) -> Option<AgentId> {
        self.find_by_capability(caps).into_iter().next()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::*;
    use std::collections::HashSet;

    fn make_agent(name: &str, caps: impl IntoIterator<Item = Capability>) -> AgentId {
        AgentId {
            name: name.into(),
            capabilities: caps.into_iter().collect::<HashSet<_>>(),
            trust_level: TrustLevel::Verified,
            endpoint: Endpoint::Stdio,
        }
    }

    #[test]
    fn register_and_lookup() {
        let reg = AgentRegistry::new();
        let agent = make_agent("worker-1", [Capability::ComputeCpu]);
        reg.register(agent.clone());
        let found = reg.lookup("worker-1").unwrap();
        assert_eq!(found, agent);
    }

    #[test]
    fn lookup_missing() {
        let reg = AgentRegistry::new();
        assert!(reg.lookup("nonexistent").is_none());
    }

    #[test]
    fn find_by_capability_single() {
        let reg = AgentRegistry::new();
        reg.register(make_agent("gpu-1", [Capability::ComputeGpu]));
        reg.register(make_agent("cpu-1", [Capability::ComputeCpu]));

        let gpu_agents = reg.find_by_capability(&[Capability::ComputeGpu]);
        assert_eq!(gpu_agents.len(), 1);
        assert_eq!(gpu_agents[0].name, "gpu-1");
    }

    #[test]
    fn find_by_capability_multiple() {
        let reg = AgentRegistry::new();
        reg.register(make_agent(
            "full",
            [Capability::ComputeGpu, Capability::CodeExecution],
        ));
        reg.register(make_agent("partial", [Capability::ComputeGpu]));

        let found =
            reg.find_by_capability(&[Capability::ComputeGpu, Capability::CodeExecution]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "full");
    }

    #[test]
    fn find_by_capability_no_match() {
        let reg = AgentRegistry::new();
        reg.register(make_agent("cpu-1", [Capability::ComputeCpu]));
        let found = reg.find_by_capability(&[Capability::WebSearch]);
        assert!(found.is_empty());
    }

    #[test]
    fn find_any_returns_one() {
        let reg = AgentRegistry::new();
        reg.register(make_agent("a", [Capability::ComputeCpu]));
        reg.register(make_agent("b", [Capability::ComputeCpu]));
        let found = reg.find_any(&[Capability::ComputeCpu]);
        assert!(found.is_some());
    }

    #[test]
    fn find_any_no_match() {
        let reg = AgentRegistry::new();
        assert!(reg.find_any(&[Capability::FileAccess]).is_none());
    }
}
